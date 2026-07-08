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

**Parameter parity (default config).** To isolate ARCHITECTURE from SIZE, the default width
(``d_model=48``, ``ff=128``, 2 layers) is tuned so the TOTAL trainable params (~138k) match the
baseline ``EntityExtractor``+MLP policy (~142k) within ~3% — the runs differ in *kind* of computation,
not budget. Both spend ~65.5k on the shared 4096×16 grp-id embedding; the remaining ~72k goes to
2 relation-biased attention layers + attention pooling + the pointer head here, vs DeepSets row-MLPs +
a fixed [64,64] actor/critic MLP + an indexed action head in the baseline. The wider ``d_model=256``
config (~1.56M) is the deliberate SIZE-scaling experiment, run only after the parity comparison lands.
"""

from __future__ import annotations

import numpy as np
import torch
import torch.nn as nn
from sb3_contrib.common.maskable.policies import MaskableActorCriticPolicy
from stable_baselines3.common.torch_layers import BaseFeaturesExtractor

from mtgenv_gym.codec_layout import slot_layout  # the canonical Python-side mirror of codec's buckets

# ── codec Discrete(ACTION_DIM) slot layout ───────────────────────────────────────────────────────
# The entity-slot bases are DERIVED from the table sizes + engine action_dim (via `slot_layout`), not
# hard-coded — so the MAX_PERM/ACTION_DIM contract (v1 98/32, v2 322/256, …) flows straight through and
# a checkpoint loads under whatever spec it was saved with. obs.rs bf_feat relation-id columns
# (Tier-3) are absolute F_PERM-tail indices, unchanged across the MAX_PERM bump.
_BF_INSTANCE_ID, _BF_BLOCKING_ID, _BF_ATTACHED_ID = 45, 46, 47
_N_RELATION_COLS = 3  # the trailing id columns, sliced OUT of the content projection

_TABLES = ("bf", "hand", "stack")


def _entity_slots(mh: int, mp: int, ms: int, action_dim: int) -> dict:
    """{table_name: (base, count)} for the entity-pointing buckets (bf←PERM, hand←HAND, stack←STACK),
    derived from the table sizes + engine action_dim through the shared codec mirror."""
    lay = slot_layout(max_hand=mh, max_perm=mp, max_stack=ms, action_dim=action_dim)
    return {"bf": lay["perm"], "hand": lay["hand"], "stack": lay["stack"]}


def _abstract_slot_indices(entity_slots: dict, action_dim: int) -> list:
    """Action slots that do NOT point at an entity row (COMMIT/PLAYER/MODE/COLOR/NUMBER/YES/NO)."""
    entity = set()
    for base, count in entity_slots.values():
        entity.update(range(base, base + count))
    return [i for i in range(action_dim) if i not in entity]


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

    def __init__(self, obs_space, *, d_model=48, nhead=4, ff=128, layers=2, id_embed=16, vocab=4096,
                 gather_present=True):
        super().__init__()
        self.d_model = d_model
        self.vocab = vocab
        # Perf: run the attention layers over ONLY the present rows (packed across the batch, block-
        # diagonal so tokens attend only within their own sample) instead of the full N=1+256+16+8=281
        # tokens where ~95% are padding. Mathematically identical to the full masked attention for the
        # PRESENT rows (padded keys are -inf either way); the old full path stays available via this
        # flag for the equivalence gate. See `_encode_packed` / `_encode_full`.
        self.gather_present = gather_present
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
        # Normalize the contextualized entity embeddings before the pointer dot-product so the entity
        # action logits (q·emb) sit on the SAME scale as the abstract-slot logits (q·abstract_q).
        # Without this the raw transformer-output magnitudes make entity logits ~−10 vs abstract ~0, so
        # the policy never samples an entity action (attack/cast) and PPO can't learn — the pointer must
        # be scale-balanced. Paired with the √d scaling in PointerHead.
        self.out_norm = nn.LayerNorm(d_model)

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
        B, N, _ = H.shape
        gpres = torch.ones(B, 1, dtype=torch.bool, device=H.device)
        pres = torch.cat([gpres, *present], dim=1)                   # (B, N) True = present
        mp = obs["bf_feat"].shape[1]                                 # actual bf row count (MAX_PERM)
        block_adj, att_adj = self._bf_adjacency(obs["bf_feat"])      # (B, MP, MP) each
        rel = self.w_block * block_adj + self.w_attach * att_adj     # (B, MP, MP) additive relation bias

        # Run the L attention layers. Both paths are numerically identical on the PRESENT rows (the only
        # rows the pooler + pointer read for present entities); they differ only on padded rows, whose
        # embeddings feed action logits that are always masked illegal (padded rows are never legal
        # actions). The pooled state is identical (padded keys are masked in the pool either way).
        H = (self._encode_packed if self.gather_present else self._encode_full)(H, pres, rel, mp)

        kpm = ~pres                                                  # True = padded key (ignore in pool)
        q = self.query.expand(B, -1, -1)
        pooled, _ = self.pool(q, H, H, key_padding_mask=kpm, need_weights=False)
        state = self.pool_norm(pooled.squeeze(1))                    # (B, d)
        # split the contextualized entity tokens back out (skip the globals token at index 0) and
        # normalize them for the pointer head (scale-balance with the abstract-slot queries).
        Hn = self.out_norm(H)
        off = 1
        ctx = {}
        for name in _TABLES:
            r = embs[name].shape[1]
            ctx[name] = Hn[:, off:off + r, :]
            off += r
        return state, ctx["bf"], ctx["hand"], ctx["stack"]

    def _full_bias(self, pres, rel, mp):
        """The full (B, N, N) additive attention mask: relation bias on the bf-bf block + -inf on padded
        key columns (the globals token, col 0, is always present so no query row is all -inf)."""
        B, N = pres.shape
        bias = torch.zeros(B, N, N, device=pres.device, dtype=rel.dtype)
        bias[:, 1:1 + mp, 1:1 + mp] = rel
        return bias.masked_fill((~pres).unsqueeze(1), float("-inf"))

    def _encode_full(self, H, pres, rel, mp):
        """Baseline path: L attention layers over ALL N tokens with the full (B, N, N) bias. O(B·N²)."""
        bias = self._full_bias(pres, rel, mp)
        for layer in self.layers:
            H = layer(H, bias)
        return H

    def _encode_packed(self, H, pres, rel, mp):
        """Fast path (identical on present rows): index-select the present rows across the whole batch
        into one packed sequence and run the L layers over it with a block-diagonal bias (attend only
        within the same sample) plus the same bf-bf relation bias in packed coordinates, then scatter
        the outputs back to their fixed (sample, row) positions. Padded rows keep their pre-layer token
        (their post-encoder value is never read for a present entity, and their logits are masked).
        O(B·(mean present)²) ≪ O(B·N²) when ~95% of rows are padding."""
        B, N, d = H.shape
        Hflat = H.reshape(B * N, d)
        idx = pres.reshape(-1).nonzero(as_tuple=False).squeeze(1)      # (T,) present-token indices
        Hp = Hflat.index_select(0, idx).unsqueeze(0)                   # (1, T, d) packed present tokens
        sample = torch.div(idx, N, rounding_mode="floor")             # (T,) which sample each came from
        pos = idx - sample * N                                         # (T,) its position within [0, N)
        is_bf = (pos >= 1) & (pos < 1 + mp)                            # (T,) is it a battlefield row
        bf_row = (pos - 1).clamp(min=0, max=mp - 1)                   # (T,) bf-row index (clamped; masked if !bf)

        T = idx.shape[0]
        same = sample.unsqueeze(0) == sample.unsqueeze(1)             # (T, T) same-sample pairs
        bias = torch.zeros(T, T, device=H.device, dtype=rel.dtype)
        bias.masked_fill_(~same, float("-inf"))                      # block-diagonal: no cross-sample attn
        # add the bf-bf relation bias for same-sample battlefield pairs: rel[sample_i, bf_row_i, bf_row_j].
        relpack = rel[sample.unsqueeze(1).expand(T, T),
                      bf_row.unsqueeze(1).expand(T, T),
                      bf_row.unsqueeze(0).expand(T, T)]               # (T, T) gathered
        bf_pair = is_bf.unsqueeze(1) & is_bf.unsqueeze(0) & same
        bias = bias + torch.where(bf_pair, relpack, torch.zeros_like(relpack))

        bias = bias.unsqueeze(0)                                       # (1, T, T) — batch dim for the layer
        for layer in self.layers:
            Hp = layer(Hp, bias)
        Hnew = Hflat.index_copy(0, idx, Hp.squeeze(0))                # scatter present outputs back
        return Hnew.reshape(B, N, d)


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

    def __init__(self, observation_space, d_model=48, nhead=4, ff=128, layers=2):
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

    def __init__(self, pack: _PackSpec, action_dim: int):
        super().__init__()
        self.pack = pack
        self.action_dim = int(action_dim)
        d = pack.d
        self.q_proj = nn.Linear(pack.state_dim, d)
        self.scale = d ** -0.5           # √d scaling so logits are O(1) (entity emb + abstract_q unit-var)
        # Entity/abstract split derived from the table sizes + action_dim (no hard-coded bucket bases).
        self._entity_slots = _entity_slots(pack.mh, pack.mp, pack.ms, self.action_dim)
        self._abstract_idx = _abstract_slot_indices(self._entity_slots, self.action_dim)
        self.abstract_q = nn.Parameter(torch.randn(len(self._abstract_idx), d))  # unit-var, like out_norm emb

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
        q = self.q_proj(state) * self.scale                          # (B, d), √d-scaled
        logits = torch.zeros(B, self.action_dim, device=feats.device, dtype=feats.dtype)
        for name, (base, count) in self._entity_slots.items():
            emb = ctx[name][:, :count, :]                            # (B, count, d) — out_norm'd, unit-var
            logits[:, base:base + count] = torch.einsum("bd,bcd->bc", q, emb)
        logits[:, self._abstract_idx] = torch.einsum("bd,ad->ba", q, self.abstract_q)
        return logits


class ValueHead(nn.Module):
    """Flat features → scalar value, from the pooled state slice only. Hidden scales with the model
    width (``state_dim``) so the head stays a fixed fraction of the budget as the arch is narrowed."""

    def __init__(self, pack: _PackSpec, hidden=None):
        super().__init__()
        self.pack = pack
        hidden = hidden or pack.state_dim
        self.net = nn.Sequential(nn.Linear(pack.state_dim, hidden), nn.GELU(), nn.Linear(hidden, 1))

    def forward(self, feats):
        return self.net(feats[:, self.pack.o_state:self.pack.o_state + self.pack.state_dim])


class RelationalPointerPolicy(MaskableActorCriticPolicy):
    """MaskablePPO policy: ``RelationalAttnExtractor`` + pointer/value heads, SB3 masking/PPO unchanged."""

    def __init__(self, *args, d_model=48, nhead=4, ff=128, layers=2, **kwargs):
        kwargs["features_extractor_class"] = RelationalAttnExtractor
        kwargs["features_extractor_kwargs"] = dict(d_model=d_model, nhead=nhead, ff=ff, layers=layers)
        kwargs["net_arch"] = []                       # identity mlp_extractor: latent = features
        super().__init__(*args, **kwargs)

    def _build(self, lr_schedule):
        super()._build(lr_schedule)                   # builds extractor + (soon-replaced) action/value nets
        pack = self.features_extractor.pack
        self.action_net = PointerHead(pack, int(self.action_space.n))
        self.value_net = ValueHead(pack)
        self.optimizer = self.optimizer_class(self.parameters(), lr=lr_schedule(1.0),
                                              **self.optimizer_kwargs)
