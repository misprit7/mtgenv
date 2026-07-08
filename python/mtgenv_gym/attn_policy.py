"""Relational attention encoder + pointer (action-as-input) policy for MaskablePPO.

The hypothesis this arm tests: a **relational encoder** (self-attention over all entities with a hard
adjacency bias from the engine's relation ids) plus a **content-based pointer head** (an action slot's
logit is a query dotted with the *contextualized embedding of the entity it points at*) makes
combat-judgment lines — double-block the trampler, don't chump, don't attack a lone bear into a swine —
*expressible*, where the baseline mean-pool + fixed indexed head cannot represent "score THIS action by
the object it targets, in the context of what's blocking what."

Three pieces:
  * ``RelationalAttnExtractor`` — tokens = every bf/hand/stack entity row + a globals token; L pre-norm
    self-attention layers whose attention scores get an **additive bias on related pairs** (blocker↔the
    attacker it blocks, aura↔its host) built by MATCHING the obs relation-id columns
    (``instance_id``/``blocking_id``/``attached_to_id``, obs.rs Tier-3) — so the net is handed the graph
    instead of having to discover id-equality through Q/K. State summary = a learned query attending
    over the tokens (attention pooling, not a mean). The relation-id columns are used ONLY for adjacency
    and are sliced OUT of the content projection (raw ids don't embed).
  * ``PointerHead`` — entity-pointing action slots (HAND[i]/PERM[i]/STACK[i]) score ``q · entity_emb[i]``
    on the POST-attention embeddings; abstract slots (COMMIT/PLAYER/MODE/COLOR/NUMBER/YES/NO) get learned
    per-slot queries dotted with the same ``q``. Masking is unchanged (SB3 applies it to these logits).
  * ``RelationalPointerPolicy`` — a ``MaskableActorCriticPolicy`` with ``net_arch=[]`` (identity
    mlp_extractor) whose ``action_net``/``value_net`` are replaced by the pointer/value heads, so SB3's
    masking, sampling, GAE and PPO update are reused verbatim.

The extractor packs ``[state, bf_emb, hand_emb, stack_emb]`` into the flat features tensor SB3 expects;
the heads slice it back apart (offsets in ``_PackSpec``).
"""

from __future__ import annotations

import numpy as np
import torch
import torch.nn as nn
from sb3_contrib.common.maskable.policies import MaskableActorCriticPolicy
from stable_baselines3.common.torch_layers import BaseFeaturesExtractor

# ── codec Discrete(ACTION_DIM) slot layout (mirrors crate::codec) ────────────────────────────────
_COMMIT = 0
_HAND_BASE, _MAX_HAND = 1, 16
_PERM_BASE, _MAX_PERM = 17, 32
_PLAYER_BASE, _N_PLAYER = 49, 2
_STACK_BASE, _MAX_STACK = 51, 8
_ACTION_DIM = 98
# obs.rs bf_feat relation-id columns (Tier-3, append-stable absolute indices).
_BF_INSTANCE_ID, _BF_BLOCKING_ID, _BF_ATTACHED_ID = 45, 46, 47
_N_RELATION_COLS = 3  # the trailing id columns, sliced OUT of the content projection

_TABLES = ("bf", "hand", "stack")
# which action-slot bucket points at each entity table, and that table's row count.
_ENTITY_SLOTS = {"bf": (_PERM_BASE, _MAX_PERM), "hand": (_HAND_BASE, _MAX_HAND),
                 "stack": (_STACK_BASE, _MAX_STACK)}


def _abstract_slot_indices() -> list:
    """Action slots that do NOT point at an entity row (COMMIT/PLAYER/MODE/COLOR/NUMBER/YES/NO)."""
    entity = set()
    for base, count in _ENTITY_SLOTS.values():
        entity.update(range(base, base + count))
    return [i for i in range(_ACTION_DIM) if i not in entity]


# ── relation-biased transformer layer ────────────────────────────────────────────────────────────
class _RelBiasLayer(nn.Module):
    """Pre-norm self-attention + FFN, with an additive per-pair attention bias (the relation graph)."""

    def __init__(self, d_model, nhead, ff, dropout=0.0):
        super().__init__()
        self.nhead = nhead
        self.attn = nn.MultiheadAttention(d_model, nhead, dropout=dropout, batch_first=True)
        self.norm1 = nn.LayerNorm(d_model)
        self.norm2 = nn.LayerNorm(d_model)
        self.ff = nn.Sequential(nn.Linear(d_model, ff), nn.GELU(), nn.Linear(ff, d_model))

    def forward(self, x, attn_bias):
        # attn_bias: (B, N, N) additive float — carries BOTH the relation bias AND padding (-inf on
        # padded keys), so we pass a single float mask (no bool key_padding_mask → no type-mismatch).
        b, n, _ = x.shape
        bias = attn_bias.unsqueeze(1).expand(b, self.nhead, n, n).reshape(b * self.nhead, n, n)
        h = self.norm1(x)
        a, _ = self.attn(h, h, h, attn_mask=bias, need_weights=False)
        x = x + a
        x = x + self.ff(self.norm2(x))
        return x


class _RelationalEncoder(nn.Module):
    """Entity + globals tokens → contextualized per-entity embeddings + an attention-pooled state."""

    def __init__(self, obs_space, *, d_model=256, nhead=4, ff=512, layers=2, id_embed=16, vocab=4096):
        super().__init__()
        self.d_model = d_model
        self.vocab = vocab
        self.id_embed = nn.Embedding(vocab, id_embed)
        self.proj = nn.ModuleDict()
        self.cardid_dims = {}
        self.content_dims = {}
        for name in _TABLES:
            fdim = obs_space[f"{name}_feat"].shape[-1]
            if name == "bf":
                fdim -= _N_RELATION_COLS   # relation ids are match-keys, not content
            cid = f"{name}_cardid"
            cdim = obs_space[cid].shape[-1] if cid in obs_space.spaces else 0
            self.cardid_dims[name] = cdim
            self.content_dims[name] = fdim
            self.proj[name] = nn.Linear(fdim + id_embed + cdim, d_model)
        self.type_emb = nn.Embedding(len(_TABLES), d_model)
        g = obs_space["globals"].shape[0]
        dcid = obs_space["decision_cardid"].shape[-1] if "decision_cardid" in obs_space.spaces else 0
        self.globals_proj = nn.Linear(g + dcid, d_model)
        self.layers = nn.ModuleList([_RelBiasLayer(d_model, nhead, ff) for _ in range(layers)])
        # learned per-relation-type attention-bias magnitudes (blocker↔attacker, aura↔host).
        self.w_block = nn.Parameter(torch.tensor(2.0))
        self.w_attach = nn.Parameter(torch.tensor(2.0))
        self.query = nn.Parameter(torch.randn(1, 1, d_model) * 0.02)  # attention-pooling query
        self.pool = nn.MultiheadAttention(d_model, nhead, batch_first=True)
        self.pool_norm = nn.LayerNorm(d_model)

    def _bf_adjacency(self, bf_feat):
        """(B, MP, MP) symmetric 0/1 adjacency from id-matching, per relation type."""
        inst = bf_feat[..., _BF_INSTANCE_ID].round().long()          # (B, MP)
        block = bf_feat[..., _BF_BLOCKING_ID].round().long()
        att = bf_feat[..., _BF_ATTACHED_ID].round().long()
        valid = inst != 0                                            # a real object in row j

        def match(src):  # src[i] == inst[j], j a real object
            m = (src.unsqueeze(2) == inst.unsqueeze(1)) & valid.unsqueeze(1) & (src.unsqueeze(2) != 0)
            return (m | m.transpose(1, 2)).float()

        return match(block), match(att)

    def forward(self, obs):
        B = obs["globals"].shape[0]
        tokens, present = [], []
        embs = {}
        for ti, name in enumerate(_TABLES):
            feat = obs[f"{name}_feat"]
            content = feat[..., :self.content_dims[name]]            # drop bf relation-id cols
            ids = (obs[f"{name}_ids"].long() % self.vocab)
            parts = [content, self.id_embed(ids)]
            if self.cardid_dims[name]:
                parts.append(obs[f"{name}_cardid"])
            tok = self.proj[name](torch.cat(parts, dim=-1)) + self.type_emb.weight[ti]  # (B, R, d)
            embs[name] = tok
            tokens.append(tok)
            present.append(feat[..., 0] > 0.5)                       # (B, R)
        gin = [obs["globals"]]
        if "decision_cardid" in obs:
            gin.append(obs["decision_cardid"].reshape(B, -1))
        gtok = self.globals_proj(torch.cat(gin, dim=-1)).unsqueeze(1)  # (B, 1, d)

        H = torch.cat([gtok, *tokens], dim=1)                        # (B, N, d); N = 1 + MP + MH + MS
        N = H.shape[1]
        gpres = torch.ones(B, 1, dtype=torch.bool, device=H.device)
        pres = torch.cat([gpres, *present], dim=1)                   # (B, N) True = present
        kpm = ~pres                                                  # True = padded key (ignore)

        # additive attention mask = relation bias (bf-bf) + padding (-inf on padded keys). The globals
        # token (col 0) is always present, so no query row is fully -inf (no NaN softmax).
        bias = torch.zeros(B, N, N, device=H.device, dtype=H.dtype)
        mp = _MAX_PERM
        block_adj, att_adj = self._bf_adjacency(obs["bf_feat"])      # (B, MP, MP) each
        bias[:, 1:1 + mp, 1:1 + mp] = self.w_block * block_adj + self.w_attach * att_adj
        bias = bias.masked_fill(kpm.unsqueeze(1), float("-inf"))     # -inf on padded key columns

        for layer in self.layers:
            H = layer(H, bias)

        q = self.query.expand(B, -1, -1)
        pooled, _ = self.pool(q, H, H, key_padding_mask=kpm, need_weights=False)
        state = self.pool_norm(pooled.squeeze(1))                    # (B, d)
        # split the contextualized entity tokens back out of H (skip the globals token at index 0).
        off = 1
        ctx = {}
        for name in _TABLES:
            r = embs[name].shape[1]
            ctx[name] = H[:, off:off + r, :]
            off += r
        return state, ctx["bf"], ctx["hand"], ctx["stack"]


class _PackSpec:
    """Flat-features layout shared by the extractor (writer) and the heads (readers)."""

    def __init__(self, d_model, sizes):
        self.d = d_model
        self.state_dim = d_model
        self.mp, self.mh, self.ms = sizes["bf"], sizes["hand"], sizes["stack"]
        self.o_state = 0
        self.o_bf = self.state_dim
        self.o_hand = self.o_bf + self.mp * self.d
        self.o_stack = self.o_hand + self.mh * self.d
        self.total = self.o_stack + self.ms * self.d


class RelationalAttnExtractor(BaseFeaturesExtractor):
    """SB3 features extractor wrapping ``_RelationalEncoder``; packs its outputs into one flat tensor."""

    def __init__(self, observation_space, d_model=256, nhead=4, ff=512, layers=2):
        sizes = {name: observation_space[f"{name}_feat"].shape[0] for name in _TABLES}
        pack = _PackSpec(d_model, sizes)
        super().__init__(observation_space, features_dim=pack.total)
        self.enc = _RelationalEncoder(observation_space, d_model=d_model, nhead=nhead, ff=ff, layers=layers)
        self.pack = pack
        self.d_model = d_model
        self.state_dim = pack.state_dim
        self.table_sizes = sizes

    def forward(self, obs):
        state, bf, hand, stack = self.enc(obs)
        B = state.shape[0]
        return torch.cat([state, bf.reshape(B, -1), hand.reshape(B, -1), stack.reshape(B, -1)], dim=1)


class PointerHead(nn.Module):
    """Flat features → ``(B, ACTION_DIM)`` logits. Entity slots: ``q · entity_emb``; abstract: learned
    per-slot queries dotted with ``q``. Slot k of a bucket scores that bucket's entity row k."""

    def __init__(self, pack: _PackSpec):
        super().__init__()
        self.pack = pack
        d = pack.d
        self.q_proj = nn.Linear(pack.state_dim, d)
        self._abstract_idx = _abstract_slot_indices()
        self.abstract_q = nn.Parameter(torch.randn(len(self._abstract_idx), d) * (1.0 / d ** 0.5))

    def _unpack(self, feats):
        p = self.pack
        B = feats.shape[0]
        state = feats[:, p.o_state:p.o_state + p.state_dim]
        bf = feats[:, p.o_bf:p.o_hand].reshape(B, p.mp, p.d)
        hand = feats[:, p.o_hand:p.o_stack].reshape(B, p.mh, p.d)
        stack = feats[:, p.o_stack:p.total].reshape(B, p.ms, p.d)
        return state, {"bf": bf, "hand": hand, "stack": stack}

    def forward(self, feats):
        state, ctx = self._unpack(feats)
        B = state.shape[0]
        q = self.q_proj(state)                                       # (B, d)
        logits = torch.zeros(B, _ACTION_DIM, device=feats.device, dtype=feats.dtype)
        for name, (base, count) in _ENTITY_SLOTS.items():
            emb = ctx[name][:, :count, :]                            # (B, count, d)
            logits[:, base:base + count] = torch.einsum("bd,bcd->bc", q, emb)
        logits[:, self._abstract_idx] = torch.einsum("bd,ad->ba", q, self.abstract_q)
        return logits


class ValueHead(nn.Module):
    """Flat features → scalar value, from the pooled state slice only."""

    def __init__(self, pack: _PackSpec, hidden=128):
        super().__init__()
        self.pack = pack
        self.net = nn.Sequential(nn.Linear(pack.state_dim, hidden), nn.GELU(), nn.Linear(hidden, 1))

    def forward(self, feats):
        return self.net(feats[:, self.pack.o_state:self.pack.o_state + self.pack.state_dim])


class RelationalPointerPolicy(MaskableActorCriticPolicy):
    """MaskablePPO policy: ``RelationalAttnExtractor`` + pointer/value heads, SB3 masking/PPO unchanged."""

    def __init__(self, *args, d_model=256, nhead=4, ff=512, layers=2, **kwargs):
        kwargs["features_extractor_class"] = RelationalAttnExtractor
        kwargs["features_extractor_kwargs"] = dict(d_model=d_model, nhead=nhead, ff=ff, layers=layers)
        kwargs["net_arch"] = []                       # identity mlp_extractor: latent = features
        super().__init__(*args, **kwargs)

    def _build(self, lr_schedule):
        super()._build(lr_schedule)                   # builds extractor + (soon-replaced) action/value nets
        pack = self.features_extractor.pack
        self.action_net = PointerHead(pack)
        self.value_net = ValueHead(pack)
        self.optimizer = self.optimizer_class(self.parameters(), lr=lr_schedule(1.0),
                                              **self.optimizer_kwargs)
