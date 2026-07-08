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
_N_RELATION_COLS = 3  # the trailing id columns (v2 only), sliced OUT of the content projection

# ── v3 contract additions (OBS2_DESIGN.md §7; detected by an `edges` obs key) ─────────────────────
# v3 drops *_cardid + the three bf relation-id cols (bf_feat 48→45), renames *_ids→*_grpid, and adds:
#   edges (MAX_EDGES×4: src_row, dst_row, type, k; pad rows = -1) consumed as a per-(type,direction,head)
#   attention bias — the adjacency the v2 arm rebuilt from id-equality, handed over directly; and
#   choice_feat (MAX_CHOICE×N) = content tokens for the decision's abstract options (mode/color/number/
#   bool), giving the abstract action slots content for the pointer (deletes abstract_q, the 4.7 scale
#   family). Player (you/opp) + decision tokens become always-present attention rows so the entity-index
#   space [0, N) is the shared addressing for both attention rows and edge endpoints (§7.2).
_MAX_CHOICE = 16
_N_EDGE_TYPES = 6           # §7.4 ids 0..5: BLOCKS ATTACKS ATTACHED_TO TARGETS STACK_SOURCE PENDING_PICK
_EDGE_PAD = -1
_N_SPECIAL = 3             # appended always-present rows: you, opp, decision (edge indices 280/281/282)

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
        self.nhead = nhead
        self.v3 = "edges" in obs_space.spaces        # OBS2 v3 contract (edges + choice_feat + *_grpid)
        self.ids_suffix = "_grpid" if self.v3 else "_ids"
        self.id_embed = nn.Embedding(vocab, id_embed)
        self.proj = nn.ModuleDict()
        self.cardid_dims = {}
        self.content_dims = {}
        for name in _TABLES:
            fdim = obs_space[f"{name}_feat"].shape[-1]
            if name == "bf" and not self.v3:
                fdim -= _N_RELATION_COLS   # v2: relation ids are match-keys, not content (v3 drops the cols)
            cid = f"{name}_cardid"
            cdim = obs_space[cid].shape[-1] if cid in obs_space.spaces else 0   # v3: no *_cardid → 0
            self.cardid_dims[name] = cdim
            self.content_dims[name] = fdim
            self.proj[name] = nn.Linear(fdim + id_embed + cdim, d_model)
        self.type_emb = nn.Embedding(len(_TABLES), d_model)
        g = obs_space["globals"].shape[0]
        self.layers = nn.ModuleList([_RelBiasLayer(d_model, nhead, ff) for _ in range(layers)])
        self.query = nn.Parameter(torch.randn(1, 1, d_model) * 0.02)  # attention-pooling query
        self.pool = nn.MultiheadAttention(d_model, nhead, batch_first=True)
        self.pool_norm = nn.LayerNorm(d_model)
        if self.v3:
            # you/opp/decision content tokens (projected from globals; the per-seat blocks + decision
            # one-hot + scalars all live there — exact seat-block slicing is a later refinement), the
            # choice-option projection (pointer content for abstract buckets), and the per-(type,dir,head)
            # edge attention bias that replaces v2's id-matched adjacency + w_block/w_attach.
            self.you_proj = nn.Linear(g, d_model)
            self.opp_proj = nn.Linear(g, d_model)
            self.decision_proj = nn.Linear(g, d_model)
            self.choice_proj = nn.Linear(obs_space["choice_feat"].shape[-1], d_model)
            # per-(type, direction) additive attention bias, head-uniform (fits the layer's (B,N,N) mask;
            # per-head is a flagged refinement). direction 0 = src→dst, 1 = dst→src.
            self.edge_bias = nn.Parameter(torch.zeros(_N_EDGE_TYPES, 2))
        else:
            dcid = obs_space["decision_cardid"].shape[-1] if "decision_cardid" in obs_space.spaces else 0
            self.globals_proj = nn.Linear(g + dcid, d_model)
            # learned per-relation-type attention-bias magnitudes (blocker↔attacker, aura↔host).
            self.w_block = nn.Parameter(torch.tensor(2.0))
            self.w_attach = nn.Parameter(torch.tensor(2.0))
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

    def _entity_tokens(self, obs):
        """bf/hand/stack tokens + present flags (shared v2/v3). Reads `{name}{ids_suffix}` (v2 `_ids` /
        v3 `_grpid`); the *_cardid parts appear only when the obs provides them (v2)."""
        tokens, present, embs = [], [], {}
        for ti, name in enumerate(_TABLES):
            feat = obs[f"{name}_feat"]
            content = feat[..., :self.content_dims[name]]            # v2: drop bf relation-id cols
            ids = (obs[name + self.ids_suffix].long() % self.vocab)
            parts = [content, self.id_embed(ids)]
            if self.cardid_dims[name]:
                parts.append(obs[f"{name}_cardid"])
            tok = self.proj[name](torch.cat(parts, dim=-1)) + self.type_emb.weight[ti]  # (B, R, d)
            embs[name] = tok
            tokens.append(tok)
            present.append(feat[..., 0] > 0.5)                       # (B, R)
        return tokens, present, embs

    def _pool(self, H, pres):
        """Attention-pool over present keys → (B, d) state (padded keys masked, so weights sum over
        present only — no fixed-size denominator)."""
        B = H.shape[0]
        q = self.query.expand(B, -1, -1)
        pooled, _ = self.pool(q, H, H, key_padding_mask=~pres, need_weights=False)
        return self.pool_norm(pooled.squeeze(1))

    def forward(self, obs):
        return self._forward_v3(obs) if self.v3 else self._forward_v2(obs)

    def _forward_v2(self, obs):
        B = obs["globals"].shape[0]
        tokens, present, embs = self._entity_tokens(obs)
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
        # Present-row gather vs full attention are identical on present rows (the only rows read); see
        # `_encode_packed`. Both feed the shared pool + slice.
        H = (self._encode_packed if self.gather_present else self._encode_full)(H, pres, rel, mp)
        state = self._pool(H, pres)
        Hn = self.out_norm(H)                                        # scale-balance for the pointer
        off, ctx = 1, {}                                             # skip the globals token at index 0
        for name in _TABLES:
            ctx[name] = Hn[:, off:off + embs[name].shape[1], :]
            off += embs[name].shape[1]
        return {"state": state, "bf": ctx["bf"], "hand": ctx["hand"], "stack": ctx["stack"]}

    def _forward_v3(self, obs):
        """v3: no globals token — you/opp/decision are appended as always-present rows so the attention
        row order equals the §7.2 edge index space [bf | hand | stack | you | opp | decision]. Relations
        come from the `edges` tensor as a per-(type,direction) attention bias (not id-matching)."""
        B = obs["globals"].shape[0]
        g = obs["globals"]
        tokens, present, embs = self._entity_tokens(obs)
        # you/opp/decision content tokens (projected from globals), always present.
        specials = torch.stack([self.you_proj(g), self.opp_proj(g), self.decision_proj(g)], dim=1)  # (B,3,d)
        H = torch.cat([*tokens, specials], dim=1)                    # (B, N, d); N = MP+MH+MS+3
        B, N, _ = H.shape
        spres = torch.ones(B, _N_SPECIAL, dtype=torch.bool, device=H.device)
        pres = torch.cat([*present, spres], dim=1)                   # (B, N)
        pair_bias = self._edge_bias(obs["edges"].long(), N)          # (B, N, N) additive edge bias
        H = self._encode_full_bias(H, pres, pair_bias)               # (packed-coord edge bias = follow-up)
        state = self._pool(H, pres)
        Hn = self.out_norm(H)
        off, ctx = 0, {}                                             # no globals token; bf starts at row 0
        for name in _TABLES:
            ctx[name] = Hn[:, off:off + embs[name].shape[1], :]
            off += embs[name].shape[1]
        you, opp, decision = Hn[:, off], Hn[:, off + 1], Hn[:, off + 2]  # (B, d) each
        choice = self.out_norm(self.choice_proj(obs["choice_feat"]))     # (B, MAX_CHOICE, d) pointer content
        return {"state": state, "bf": ctx["bf"], "hand": ctx["hand"], "stack": ctx["stack"],
                "you": you, "opp": opp, "decision": decision, "choice": choice}

    def _edge_bias(self, edges, N):
        """(B, N, N) additive attention bias from the edge list: edge (src_row, dst_row, type, k) adds
        edge_bias[type, 0] to logits[src, dst] and edge_bias[type, 1] to logits[dst, src] (directional);
        pad rows (src == -1) are skipped. Head-uniform (per-(type,direction)); per-head is a refinement."""
        B, _ME, _ = edges.shape
        src, dst, etype = edges[..., 0], edges[..., 1], edges[..., 2]
        valid = src >= 0
        bias = torch.zeros(B, N, N, device=edges.device, dtype=self.edge_bias.dtype)
        b_idx = torch.arange(B, device=edges.device).unsqueeze(1).expand_as(src)
        et = etype.clamp(min=0)                                      # clamp pad (-1) → 0; masked by `valid`
        fwd = self.edge_bias[et, 0][valid]
        rev = self.edge_bias[et, 1][valid]
        bi, si, di = b_idx[valid], src[valid], dst[valid]
        bias.index_put_((bi, si, di), fwd, accumulate=True)
        bias.index_put_((bi, di, si), rev, accumulate=True)
        return bias

    def _encode_full_bias(self, H, pres, pair_bias):
        """L attention layers over all N tokens with a precomputed (B, N, N) additive pair bias + -inf on
        padded keys. (The v2 path builds its bf-bf bias inline; this is the v3 edge-bias entry point.)"""
        bias = pair_bias.masked_fill((~pres).unsqueeze(1), float("-inf"))
        for layer in self.layers:
            H = layer(H, bias)
        return H

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
    """Flat-features layout shared by the extractor (writer) and the heads (readers). v3 appends the
    always-present you/opp/decision token embeddings + the projected choice rows (pointer content for
    the player / commit / abstract-modal action slots)."""

    def __init__(self, d_model, sizes, v3=False):
        self.d = d_model
        self.state_dim = d_model
        self.mp, self.mh, self.ms = sizes["bf"], sizes["hand"], sizes["stack"]
        self.v3 = v3
        self.o_state = 0
        self.o_bf = self.state_dim
        self.o_hand = self.o_bf + self.mp * self.d
        self.o_stack = self.o_hand + self.mh * self.d
        self.total = self.o_stack + self.ms * self.d
        if v3:
            self.o_you = self.total
            self.o_opp = self.o_you + self.d
            self.o_decision = self.o_opp + self.d
            self.o_choice = self.o_decision + self.d
            self.total = self.o_choice + _MAX_CHOICE * self.d


class RelationalAttnExtractor(BaseFeaturesExtractor):
    """SB3 features extractor wrapping ``_RelationalEncoder``; packs its outputs into one flat tensor."""

    def __init__(self, observation_space, d_model=48, nhead=4, ff=128, layers=2):
        sizes = {name: observation_space[f"{name}_feat"].shape[0] for name in _TABLES}
        pack = _PackSpec(d_model, sizes, v3="edges" in observation_space.spaces)
        super().__init__(observation_space, features_dim=pack.total)
        self.enc = _RelationalEncoder(observation_space, d_model=d_model, nhead=nhead, ff=ff, layers=layers)
        self.pack = pack
        self.d_model = d_model
        self.state_dim = pack.state_dim
        self.table_sizes = sizes

    def forward(self, obs):
        out = self.enc(obs)
        B = out["state"].shape[0]
        parts = [out["state"], out["bf"].reshape(B, -1), out["hand"].reshape(B, -1),
                 out["stack"].reshape(B, -1)]
        if self.pack.v3:
            parts += [out["you"], out["opp"], out["decision"], out["choice"].reshape(B, -1)]
        return torch.cat(parts, dim=1)


class PointerHead(nn.Module):
    """Flat features → ``(B, ACTION_DIM)`` logits. Entity slots: ``q · entity_emb``; abstract: learned
    per-slot queries dotted with ``q``. Slot k of a bucket scores that bucket's entity row k."""

    def __init__(self, pack: _PackSpec, action_dim: int):
        super().__init__()
        self.pack = pack
        self.action_dim = int(action_dim)
        d = pack.d
        self.q_proj = nn.Linear(pack.state_dim, d)
        self.scale = d ** -0.5           # √d scaling so logits are O(1) (unit-var content embeddings)
        # Entity/abstract split derived from the table sizes + action_dim (no hard-coded bucket bases).
        self._entity_slots = _entity_slots(pack.mh, pack.mp, pack.ms, self.action_dim)
        if pack.v3:
            # v3: every slot is a content pointer (player/commit/modal buckets get real content tokens),
            # so the whole abstract_q family — the 4.7 unnormalized-query scale bug — is deleted.
            self._lay = slot_layout(pack.mh, pack.mp, pack.ms, self.action_dim)
        else:
            self._abstract_idx = _abstract_slot_indices(self._entity_slots, self.action_dim)
            self.abstract_q = nn.Parameter(torch.randn(len(self._abstract_idx), d))  # unit-var

    def _unpack(self, feats):
        p = self.pack
        B = feats.shape[0]
        state = feats[:, p.o_state:p.o_state + p.state_dim]
        bf = feats[:, p.o_bf:p.o_bf + p.mp * p.d].reshape(B, p.mp, p.d)
        hand = feats[:, p.o_hand:p.o_hand + p.mh * p.d].reshape(B, p.mh, p.d)
        stack = feats[:, p.o_stack:p.o_stack + p.ms * p.d].reshape(B, p.ms, p.d)
        ctx = {"bf": bf, "hand": hand, "stack": stack}
        if p.v3:
            ctx["you"] = feats[:, p.o_you:p.o_you + p.d]
            ctx["opp"] = feats[:, p.o_opp:p.o_opp + p.d]
            ctx["decision"] = feats[:, p.o_decision:p.o_decision + p.d]
            ctx["choice"] = feats[:, p.o_choice:p.o_choice + _MAX_CHOICE * p.d].reshape(B, _MAX_CHOICE, p.d)
        return state, ctx

    def forward(self, feats):
        state, ctx = self._unpack(feats)
        B = state.shape[0]
        q = self.q_proj(state) * self.scale                          # (B, d), √d-scaled
        logits = torch.zeros(B, self.action_dim, device=feats.device, dtype=feats.dtype)
        for name, (base, count) in self._entity_slots.items():       # HAND/PERM/STACK ← entity rows
            logits[:, base:base + count] = torch.einsum("bd,bcd->bc", q, ctx[name][:, :count, :])
        if not self.pack.v3:
            logits[:, self._abstract_idx] = torch.einsum("bd,ad->ba", q, self.abstract_q)
            return logits
        lay = self._lay
        pbase, pcnt = lay["player"]                                   # PLAYER ← [you, opp] tokens
        players = torch.stack([ctx["you"], ctx["opp"]], dim=1)       # (B, 2, d)
        logits[:, pbase:pbase + pcnt] = torch.einsum("bd,bcd->bc", q, players[:, :pcnt, :])
        logits[:, lay["commit"][0]] = (q * ctx["decision"]).sum(-1)  # COMMIT ← decision token
        choice = ctx["choice"]                                        # MODE/COLOR/NUMBER ← choice rows j→j
        for bucket in ("mode", "color", "number"):
            base, cnt = lay[bucket]
            k = min(cnt, choice.shape[1])
            logits[:, base:base + k] = torch.einsum("bd,bcd->bc", q, choice[:, :k, :])
        logits[:, lay["yes"][0]] = (q * choice[:, 0]).sum(-1)        # YES/NO ← choice rows 0, 1 (bool family)
        logits[:, lay["no"][0]] = (q * choice[:, 1]).sum(-1)
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
