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
        self.tables = []
        for name in _TABLES:
            feat_key = f"{name}_feat"
            if feat_key not in observation_space.spaces:
                continue
            feat_dim = observation_space[feat_key].shape[-1]
            self.row_mlps[name] = nn.Sequential(
                nn.Linear(feat_dim + embed_dim, hidden), nn.ReLU()
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
            x = torch.cat([feat, emb], dim=-1)               # (B, R, F+E)
            h = self.row_mlps[name](x)                       # (B, R, H)
            present = feat[..., :1]                          # (B, R, 1) — row 0 feature is "present"
            summed = (h * present).sum(dim=1)                # (B, H)
            count = present.sum(dim=1).clamp(min=1.0)        # (B, 1)
            pooled.append(summed / count)
        g = obs["globals"]                                   # (B, G)
        return self.head(torch.cat([g, *pooled], dim=-1))
