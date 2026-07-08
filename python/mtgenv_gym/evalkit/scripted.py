"""``ScriptedPolicy`` — a fixed, deterministic reference policy for heralds (and any deck).

The **known-optimal-ish yardstick** the user defined: its measured win-rate vs random is the realistic
vs-random ceiling, and `selfplay/winrate_vs_script` (agent vs this script) is the standing per-run
metric — a learned agent scoring ≈0.5 against it means it has learned the deck (a lower-noise signal
than the saturating vs-random curve).

Decision priority at every point, using only the legal actions in the mask:
  (a) **play a land** if one is available;
  (b) else **cast a spell** if one is available;
  (c) else at **declare-attackers**, attack with EVERYTHING (keep taking an attacker toggle while any
      is available, then commit);
  (d) at **declare-blockers**, NEVER block — commit immediately with no blocks;
  (e) otherwise prefer the **pass/commit** action; failing that, **decline** (NO — e.g. mulligan-keep /
      confirm-no); failing that, the **first legal** action.
Within any category the first (lowest-index) available slot wins (the user's explicit tie-break).

Torch-free and batched (implements the evalkit ``Policy`` protocol), so it runs as an Arena opponent
in any venv. It reads the observation to classify legal action slots semantically, using the two
data-only seams it can rely on without matching card names:

* the **codec slot layout** (``crates/mtg-py/src/codec.rs`` — a flat ``Discrete(98)`` whose buckets are
  positional: ``COMMIT``, ``HAND[i]``, ``PERM[i]``, ``PLAYER[i]``, ``STACK[i]``, …), and
* the **observation encoding** (``crates/mtg-py/src/obs.rs``): the current decision KIND is a one-hot
  inside ``globals``, and each ``hand_feat`` row carries a card-type one-hot (so "is this hand card a
  land" is recoverable without knowing the card).

(``tracked_stats`` classifies the engine's *post-hoc* ``decision_stats`` records, not pre-decision mask
slots, so it can't drive action selection — hence classification off the codec/obs layout here.)
"""

from __future__ import annotations

import numpy as np

from .policy import BasePolicy

# ── codec Discrete(ACTION_DIM) slot layout (mirrors crate::codec; pinned by test vs PyGame) ──────────
COMMIT = 0
HAND_BASE, MAX_HAND = 1, 16
PERM_BASE, MAX_PERM = 17, 32
PLAYER_BASE = PERM_BASE + MAX_PERM          # 49
STACK_BASE = PLAYER_BASE + 2                # 51
YES, NO = 96, 97

# ── obs `globals` decision-kind one-hot (obs.rs::encode_globals) ─────────────────────────────────────
# Layout up to it: turn(1) + phases(12) + [active,priority,priority_some](3) + my-seat(13) + opp-seat(13)
# + stack-depth(1) = 43, then the NUM_REQUESTS-wide request one-hot. Indices are `request_index()`.
DECISION_ONEHOT_OFF = 43
NUM_REQUESTS = 21
R_PRIORITY = 2
R_DECLARE_ATTACKERS = 9
R_DECLARE_BLOCKERS = 10

# ── hand_feat row layout (obs.rs::encode_hand, F_HAND=18) ────────────────────────────────────────────
# [present, mana_value, castable, card_types(8), colors(5), is_src, is_cand]; CARD_TYPES[1] == "Land",
# and the card-type block starts at index 3, so the land flag is column 3 + 1 = 4.
HAND_LAND_COL = 4


class ScriptedPolicy(BasePolicy):
    """Deterministic land > spell > attack-all > never-block > pass reference (see module docstring).

    ``mode`` is ignored — the script has no sampling (greedy == sample), which is exactly why it's a
    stable opponent for the ``winrate_vs_script`` metric. Stateless, so ``reset`` no-ops (BasePolicy)."""

    def act(self, obs_batch, mask_batch, *, mode="greedy", env_indices=None):
        out = np.empty(len(mask_batch), dtype=np.int64)
        for k in range(len(mask_batch)):
            out[k] = self._choose(obs_batch[k], np.asarray(mask_batch[k], dtype=bool))
        return out

    def _choose(self, obs, mask) -> int:
        legal = np.flatnonzero(mask)
        assert legal.size, "ScriptedPolicy given an empty action mask"
        g = np.asarray(obs["globals"]).reshape(-1)
        ridx = int(np.argmax(g[DECISION_ONEHOT_OFF:DECISION_ONEHOT_OFF + NUM_REQUESTS]))

        if ridx == R_DECLARE_BLOCKERS:                       # (d) never block → commit no blocks
            return COMMIT if mask[COMMIT] else int(legal[0])

        if ridx == R_DECLARE_ATTACKERS:                      # (c) attack with everything
            noncommit = legal[legal != COMMIT]               # an attacker/defender toggle, if any left
            return int(noncommit[0]) if noncommit.size else COMMIT

        if ridx == R_PRIORITY:
            hand = np.asarray(obs["hand_feat"])              # (MAX_HAND, F_HAND); legal HAND slot ⟹ playable
            hand_slots = [s for s in legal if HAND_BASE <= s < HAND_BASE + MAX_HAND]
            lands = [s for s in hand_slots if hand[s - HAND_BASE, HAND_LAND_COL] > 0.5]
            if lands:                                        # (a) play a land
                return min(lands)
            spells = [s for s in hand_slots if s not in lands]
            if spells:                                       # (b) cast a spell
                return min(spells)
            # nothing to play (only activations / pass available) → pass.
            return self._fallback(mask, legal)

        return self._fallback(mask, legal)                   # (e) any other decision

    @staticmethod
    def _fallback(mask, legal) -> int:
        """Prefer pass/commit; else decline (NO — mulligan-keep / confirm-no, never mulligan-to-death);
        else the first legal slot."""
        if mask[COMMIT]:
            return COMMIT
        if mask[NO]:
            return NO
        return int(legal[0])
