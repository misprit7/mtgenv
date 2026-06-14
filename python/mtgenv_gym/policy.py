"""A DeepSets-style features extractor for the structured MTG observation (GYM_PLAN §3).

The observation is a dict of fixed-shape tensors: ``globals`` plus three entity tables
(``bf``/``hand``/``stack``), each a ``(rows, feat)`` float array and a ``(rows,)`` integer
``grp_id`` array. This extractor:

1. embeds each row's ``grp_id`` (hashed into a fixed table — a never-seen printing just lands in a
   bucket, so the policy generalizes across a growing pool without a reshape, GYM_PLAN §3);
2. concatenates the embedding with the row's other features and runs a shared per-row MLP;
3. **masked mean-pools** over present rows (the per-row ``present`` flag is feature 0), giving a
   permutation-invariant summary of each variable-length table;
4. concatenates the three pooled vectors with ``globals`` → the feature vector the actor/critic
   heads consume.

Permutation invariance matters: the battlefield/hand/stack are unordered sets, and pooling means
the policy doesn't overfit to row position (which is just the engine's view order).
"""

from __future__ import annotations

import torch
import torch.nn as nn
from stable_baselines3.common.torch_layers import BaseFeaturesExtractor

_TABLES = ["bf", "hand", "stack"]


class EntityExtractor(BaseFeaturesExtractor):
    def __init__(self, observation_space, embed_dim=16, vocab=4096, hidden=64, features_dim=128):
        super().__init__(observation_space, features_dim=features_dim)
        self.vocab = vocab
        self.embed = nn.Embedding(vocab, embed_dim)

        self.row_mlps = nn.ModuleDict()
        self.cardid_dims = {}
        self.tables = []
        for name in _TABLES:
            feat_key = f"{name}_feat"
            if feat_key not in observation_space.spaces:
                continue
            feat_dim = observation_space[feat_key].shape[-1]
            # Deck-determined card-identity one-hot (env-side seam), fed per row alongside the hashed
            # grp_id embedding. 0 when the env doesn't provide it (backward compatible).
            cid_key = f"{name}_cardid"
            cardid_dim = observation_space[cid_key].shape[-1] if cid_key in observation_space.spaces else 0
            self.cardid_dims[name] = cardid_dim
            self.row_mlps[name] = nn.Sequential(
                nn.Linear(feat_dim + embed_dim + cardid_dim, hidden), nn.ReLU()
            )
            self.tables.append(name)

        g = observation_space["globals"].shape[0]
        self.head = nn.Sequential(
            nn.Linear(g + hidden * len(self.tables), features_dim), nn.ReLU()
        )

    def forward(self, obs):
        pooled = []
        for name in self.tables:
            feat = obs[f"{name}_feat"]                       # (B, R, F)
            ids = (obs[f"{name}_ids"].long() % self.vocab)   # (B, R)
            emb = self.embed(ids)                            # (B, R, E)
            parts = [feat, emb]
            if self.cardid_dims[name]:
                parts.append(obs[f"{name}_cardid"])          # (B, R, V) explicit card-identity one-hot
            x = torch.cat(parts, dim=-1)                     # (B, R, F+E[+V])
            h = self.row_mlps[name](x)                       # (B, R, H)
            present = feat[..., :1]                          # (B, R, 1) — row 0 feature is "present"
            summed = (h * present).sum(dim=1)                # (B, H)
            count = present.sum(dim=1).clamp(min=1.0)        # (B, 1)
            pooled.append(summed / count)
        g = obs["globals"]                                   # (B, G)
        return self.head(torch.cat([g, *pooled], dim=-1))


class AttnEntityExtractor(BaseFeaturesExtractor):
    """A bigger, attention-based extractor (the A/B against ``EntityExtractor``'s mean-pool).

    Two upgrades over the DeepSets baseline, both targeting *game-playing* capacity rather than card
    count: (1) **self-attention over entities** — all bf/hand/stack rows are projected into one set
    (with a table-type embedding + a learned always-present "sink" token) and run through a
    Transformer encoder layer, so entities can *relate* (attacker↔blocker, aura↔host, threat
    assessment) instead of being averaged independently; (2) **attention pooling** — a learned query
    attends over the encoded set (masked to present rows) instead of a mean. Wider too
    (``d_model``/``features_dim``). The sink token guarantees ≥1 valid key, so the empty-board opening
    obs can't NaN the masked softmax.
    """

    def __init__(self, observation_space, embed_dim=16, vocab=4096, d_model=128, nhead=4,
                 ff=256, features_dim=256):
        super().__init__(observation_space, features_dim=features_dim)
        self.vocab = vocab
        self.embed = nn.Embedding(vocab, embed_dim)
        self.proj = nn.ModuleDict()
        self.cardid_dims = {}
        self.tables = []
        for name in _TABLES:
            if f"{name}_feat" not in observation_space.spaces:
                continue
            fdim = observation_space[f"{name}_feat"].shape[-1]
            cid = f"{name}_cardid"
            cdim = observation_space[cid].shape[-1] if cid in observation_space.spaces else 0
            self.cardid_dims[name] = cdim
            self.proj[name] = nn.Linear(fdim + embed_dim + cdim, d_model)
            self.tables.append(name)
        self.type_emb = nn.Embedding(len(self.tables), d_model)  # which table a row came from
        self.sink = nn.Parameter(torch.randn(1, 1, d_model) * 0.02)  # always-present token (no-NaN + global)
        # norm_first (pre-norm) + a LayerNorm after pooling: standard transformer-in-RL stabilizers —
        # without them the attention activations can blow up after a while and NaN the logits.
        self.encoder = nn.TransformerEncoderLayer(d_model, nhead, ff, batch_first=True, dropout=0.0,
                                                  norm_first=True)
        self.query = nn.Parameter(torch.randn(1, 1, d_model) * 0.02)  # learned pooling query
        self.pool = nn.MultiheadAttention(d_model, nhead, batch_first=True, dropout=0.0)
        self.pool_norm = nn.LayerNorm(d_model)
        g = observation_space["globals"].shape[0]
        self.head = nn.Sequential(nn.Linear(g + d_model, features_dim), nn.ReLU())

    def forward(self, obs):
        B = obs["globals"].shape[0]
        rows, present = [], []
        for ti, name in enumerate(self.tables):
            feat = obs[f"{name}_feat"]                         # (B, R, F)
            ids = (obs[f"{name}_ids"].long() % self.vocab)
            parts = [feat, self.embed(ids)]
            if self.cardid_dims[name]:
                parts.append(obs[f"{name}_cardid"])
            h = self.proj[name](torch.cat(parts, dim=-1)) + self.type_emb.weight[ti]  # (B, R, d)
            rows.append(h)
            present.append(feat[..., 0] > 0.5)                # (B, R)
        sink = self.sink.expand(B, -1, -1)                    # (B, 1, d)
        H = torch.cat([sink, *rows], dim=1)                   # (B, 1+Rtot, d)
        pad = torch.cat([torch.zeros(B, 1, dtype=torch.bool, device=H.device),
                         ~torch.cat(present, dim=1)], dim=1)  # True = padded (ignored); sink never padded
        enc = self.encoder(H, src_key_padding_mask=pad)       # entities relate
        q = self.query.expand(B, -1, -1)
        pooled, _ = self.pool(q, enc, enc, key_padding_mask=pad)  # attention pool → (B, 1, d)
        pooled = self.pool_norm(pooled.squeeze(1))                # stabilize before the head
        return self.head(torch.cat([obs["globals"], pooled], dim=-1))
