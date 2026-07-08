"""Relational attention encoder + pointer policy (python/mtgenv_gym/attn_policy.py).

Synthetic-obs coverage (no engine rebuild needed — we construct 48-wide bf_feat directly): the
relation-id adjacency construction, pointer-logit alignment (slot k scores entity k's embedding),
forward shapes / no-NaN, the 1-3M param budget, and that the full MaskablePPO policy builds.
"""

import gymnasium as gym
import numpy as np
import pytest
import torch
from gymnasium import spaces

from mtgenv_gym.attn_policy import (
    _BF_ATTACHED_ID,
    _BF_BLOCKING_ID,
    _BF_INSTANCE_ID,
    _abstract_slot_indices,
    _entity_slots,
    PointerHead,
    RelationalAttnExtractor,
    _PackSpec,
    _RelationalEncoder,
)

_V = 11  # cardid one-hot width
# Contract v2 table sizes (MAX_PERM 64). The action_dim + all bucket bases are DERIVED from these via
# `slot_layout` — the test pins the same spec-driven layout the policy builds, so a future contract
# bump only needs these three numbers.
_MAX_HAND, _MAX_PERM, _MAX_STACK = 16, 64, 8
# action_dim = COMMIT(1) + HAND + PERM + PLAYER(2) + STACK + MODE(16) + COLOR(5) + NUMBER(16) + YES/NO(2)
_ADIM = 1 + _MAX_HAND + _MAX_PERM + 2 + _MAX_STACK + 16 + 5 + 16 + 2  # = 130 for MAX_PERM 64
_SLOTS = _entity_slots(_MAX_HAND, _MAX_PERM, _MAX_STACK, _ADIM)  # {"bf":(base,cnt),"hand":..,"stack":..}


def _obs_space():
    def box(shape, hi=np.inf):
        return spaces.Box(low=-hi, high=hi, shape=shape, dtype=np.float32)
    return spaces.Dict({
        "globals": box((69,)),
        "bf_feat": box((_MAX_PERM, 48)), "bf_ids": spaces.Box(0, 1 << 30, (_MAX_PERM,), np.int64),
        "bf_cardid": box((_MAX_PERM, _V), 1.0),
        "hand_feat": box((_MAX_HAND, 18)), "hand_ids": spaces.Box(0, 1 << 30, (_MAX_HAND,), np.int64),
        "hand_cardid": box((_MAX_HAND, _V), 1.0),
        "stack_feat": box((_MAX_STACK, 18)), "stack_ids": spaces.Box(0, 1 << 30, (_MAX_STACK,), np.int64),
        "stack_cardid": box((_MAX_STACK, _V), 1.0),
        "decision_cardid": box((1, _V), 1.0), "decision_ids": spaces.Box(0, 1 << 30, (1,), np.int64),
    })


def _obs_batch(B=3):
    sp = _obs_space()
    out = {}
    for k, s in sp.spaces.items():
        if s.dtype == np.int64:
            out[k] = torch.randint(0, 50, (B, *s.shape))
        else:
            out[k] = torch.randn(B, *s.shape)
    # make some bf rows "present" (feat[...,0] > 0.5)
    out["bf_feat"][:, :4, 0] = 1.0
    return out


# ── adjacency from relation ids ────────────────────────────────────────────────────────────────
def test_bf_adjacency_matches_ids():
    enc = _RelationalEncoder(_obs_space(), d_model=32, nhead=2, ff=64, layers=1)
    bf = torch.zeros(1, _MAX_PERM, 48)
    # row0 blocks row1; row2 is an aura attached to row3. instance_ids: 10,20,30,40.
    bf[0, 0, _BF_INSTANCE_ID], bf[0, 1, _BF_INSTANCE_ID] = 10, 20
    bf[0, 2, _BF_INSTANCE_ID], bf[0, 3, _BF_INSTANCE_ID] = 30, 40
    bf[0, 0, _BF_BLOCKING_ID] = 20        # row0 blocks the object with instance 20 = row1
    bf[0, 2, _BF_ATTACHED_ID] = 40        # row2 attached to instance 40 = row3
    block_adj, att_adj = enc._bf_adjacency(bf)
    assert block_adj[0, 0, 1] == 1.0 and block_adj[0, 1, 0] == 1.0, "blocker↔attacker edge, symmetric"
    assert block_adj[0, 0, 2] == 0.0, "no spurious block edge"
    assert att_adj[0, 2, 3] == 1.0 and att_adj[0, 3, 2] == 1.0, "aura↔host edge, symmetric"
    # a zero blocking_id must not link to padding rows (instance 0).
    assert block_adj[0, 1, 5] == 0.0


# ── pointer-logit alignment ──────────────────────────────────────────────────────────────────────
def test_pointer_slot_scores_matching_entity():
    pack = _PackSpec(d_model=8, sizes={"bf": _MAX_PERM, "hand": _MAX_HAND, "stack": _MAX_STACK})
    head = PointerHead(pack, _ADIM)
    B = 2
    feats = torch.randn(B, pack.total)
    state, ctx = head._unpack(feats)
    q = head.q_proj(state) * head.scale     # √d-scaled query (pointer balances entity vs abstract logits)
    logits = head(feats)
    # entity slot k of each bucket == q · that bucket's contextual entity-emb k (exact alignment).
    for name in ("bf", "hand"):
        base, count = _SLOTS[name]
        expect = torch.einsum("bd,bcd->bc", q, ctx[name])
        assert torch.allclose(logits[:, base:base + count], expect, atol=1e-5), f"{name} pointer misaligned"
    # abstract slots filled from the learned abstract queries.
    assert logits.shape == (B, _ADIM)
    ab = _abstract_slot_indices(_SLOTS, _ADIM)
    assert torch.allclose(logits[:, ab], torch.einsum("bd,ad->ba", q, head.abstract_q), atol=1e-5)
    # entity slots and abstract slots together tile all action_dim slots exactly once.
    entity = set()
    for b, c in _SLOTS.values():
        entity |= set(range(b, b + c))
    assert entity.isdisjoint(ab) and len(entity) + len(ab) == _ADIM


# ── forward shapes + no NaN, and param budget ─────────────────────────────────────────────────────
_BASELINE_PARAMS = 142_307  # MaskableActorCriticPolicy + EntityExtractor (net_arch pi/vf=[64,64])


def test_extractor_forward_and_param_budget():
    ext = RelationalAttnExtractor(_obs_space())  # PARITY defaults: d_model=48, ff=128, 2 layers
    feats = ext(_obs_batch(3))
    assert feats.shape == (3, ext.pack.total)
    assert torch.isfinite(feats).all()
    head = PointerHead(ext.pack, _ADIM)
    logits = head(feats)
    assert logits.shape == (3, _ADIM) and torch.isfinite(logits).all()
    # Parity: total attn-policy params within ±20% of the baseline (architecture isolated from size).
    # Param count is INVARIANT to MAX_PERM (attention shares per-row projections; abstract_q count is
    # action_dim − entity_slots = 42, unchanged by the +32/+32 v2 bump), so the v1 band still holds.
    from mtgenv_gym.attn_policy import RelationalPointerPolicy, ValueHead
    from gymnasium import spaces
    pol = RelationalPointerPolicy(_obs_space(), spaces.Discrete(_ADIM), lambda _: 3e-4)
    n = sum(p.numel() for p in pol.parameters())
    assert 0.8 * _BASELINE_PARAMS <= n <= 1.2 * _BASELINE_PARAMS, \
        f"parity attn {n:,} not within ±20% of baseline {_BASELINE_PARAMS:,}"
    # the wider config is the parked SIZE experiment (~1.5M), deliberately out of the parity band.
    big = RelationalPointerPolicy(_obs_space(), spaces.Discrete(_ADIM), lambda _: 3e-4, d_model=256, ff=512)
    assert sum(p.numel() for p in big.parameters()) > 1_000_000


def test_full_maskable_policy_builds():
    from mtgenv_gym.attn_policy import RelationalPointerPolicy

    pol = RelationalPointerPolicy(_obs_space(), spaces.Discrete(_ADIM), lambda _: 3e-4)
    # action_net / value_net were replaced by the pointer / value heads; optimizer covers their params.
    from mtgenv_gym.attn_policy import PointerHead as PH, ValueHead as VH
    assert isinstance(pol.action_net, PH) and isinstance(pol.value_net, VH)
    opt_params = {id(p) for grp in pol.optimizer.param_groups for p in grp["params"]}
    assert all(id(p) in opt_params for p in pol.action_net.parameters()), "pointer head not in optimizer"
