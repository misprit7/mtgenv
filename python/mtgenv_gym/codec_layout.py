"""The canonical Python-side mirror of ``crate::codec``'s flat ``Discrete(ACTION_DIM)`` bucket layout.

Pure arithmetic (no torch, no engine import) so any policy head can derive its entity/abstract slot
split from the observation table sizes + the engine's ``action_dim`` — nothing downstream hard-codes a
bucket base or the total. Both the DMC head (``dmc.py``) and the relational pointer head
(``attn_policy.py``) import ``slot_layout`` from here; ``slot_layout`` asserts its computed total
against ``PyGame.action_dim()`` so an obs↔codec desync (or a stale MAX_PERM) is caught loudly.

Contract history: v1 was ``MAX_PERM=32 → action_dim=98``; v2 (2026-07-08) is ``MAX_PERM=256 →
action_dim=322`` (late-game truncation fix). Because the bases are derived, both flow through unchanged.
"""

from __future__ import annotations

# Abstract-bucket sizes (mirror crate::codec constants). The entity buckets (HAND/PERM/STACK) size
# from the obs table widths; these fixed ones fill the rest of the vocabulary.
_N_PLAYER_SLOTS = 2
_MAX_MODES = 16
_N_COLORS = 5
_MAX_NUM = 16


def slot_layout(max_hand: int, max_perm: int, max_stack: int, action_dim: int) -> dict:
    """The ``{bucket: (base, count)}`` map + a check that it tiles ``[0, action_dim)`` exactly as
    ``codec.rs`` lays it out (COMMIT, HAND, PERM, PLAYER, STACK, MODE, COLOR, NUMBER, YES, NO)."""
    commit = 0
    hand = commit + 1
    perm = hand + max_hand
    player = perm + max_perm
    stack = player + _N_PLAYER_SLOTS
    mode = stack + max_stack
    color = mode + _MAX_MODES
    number = color + _N_COLORS
    yes = number + _MAX_NUM
    no = yes + 1
    dim = no + 1
    assert dim == action_dim, f"slot layout total {dim} != action_dim {action_dim} (obs↔codec desync)"
    # (base, count) for EVERY bucket — the entity-backed ones (hand/perm/stack) are the content-scatter
    # targets in v2; v3 additionally points player/mode/color/number/yes/no slots at content tokens, so
    # it needs all bases. Additive: existing callers index only the keys they use.
    return {"commit": (commit, 1), "hand": (hand, max_hand), "perm": (perm, max_perm),
            "player": (player, _N_PLAYER_SLOTS), "stack": (stack, max_stack),
            "mode": (mode, _MAX_MODES), "color": (color, _N_COLORS), "number": (number, _MAX_NUM),
            "yes": (yes, 1), "no": (no, 1), "action_dim": dim}
