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


# ── bf_feat per-permanent columns (obs.rs::encode_battlefield push order; append-stable) ─────────────
# present(0) mine(1) power(2) toughness(3) mana_value(4) damage(5) tapped(6) … attacking(39) …
# blocked_by(43) = blockers ganging this attacker (committed + pending, updated mid-decision).
BF_PRESENT, BF_MINE, BF_POWER, BF_TOUGHNESS = 0, 1, 2, 3
BF_TAPPED = 6
BF_ATTACKING, BF_BLOCKED_BY = 39, 43

PLAYER_BASE = PERM_BASE + MAX_PERM                    # 49 — defender player slots at a DeclareAttackers
GANG_MIN = 2                                          # "prefer 2+ blockers on the biggest attacker"

ATTACK_MODES = ("all", "never", "conservative")
BLOCK_MODES = ("never", "all", "gang")


class ScriptedHeuristic(ScriptedPolicy):
    """The scripted reference parameterized into a benchmark FAMILY — the fixed yardsticks the ratings
    ladder hangs off. Two independent axes over the same land>spell priority as ``ScriptedPolicy``:

      * **attack** — ``all`` (attack with everything, the racer) / ``never`` (hold everything back) /
        ``conservative`` (attack a creature only if no bigger *untapped* enemy would beat it in a
        block — i.e. skip a creature when some untapped enemy has power ≥ its toughness *and*
        toughness > its power, a strictly losing attack).
      * **block** — ``never`` (take it all) / ``all`` (throw a blocker at every attacker, over-chumps)
        / ``gang`` (prefer 2+ blockers on the biggest attacker: pile blockers onto the largest
        under-blocked attacker until it has ``GANG_MIN``, then the next largest).

    Named pool members (see ``rate_agent.DEFAULT_POOLS``): script-racer = (all, never) — byte-identical
    to ``ScriptedPolicy``; script-turtle = (never, all); script-gang = (all, gang); script-careful =
    (conservative, gang). They anchor the scale AND probe blind spots (a pool that can't punish
    chump-blocking won't rate it down).

    Combat is the engine's autoregressive sub-step model (codec::Interaction): a DeclareAttackers is
    (pick attacker → pick defender)*, a DeclareBlockers is (pick blocker → pick which attacker it
    blocks)*, each ending in COMMIT. ``mask[COMMIT]`` is legal only at the top-level pick, never at a
    sub-pick, so it cleanly separates "choose an attacker/blocker" from "assign its defender/attacker".
    Gang structure so far is read from the obs ``blocked_by`` column (updated mid-decision), so the
    heuristic stays stateless. Non-combat decisions defer to ``ScriptedPolicy`` unchanged."""

    def __init__(self, attack: str = "all", block: str = "never"):
        if attack not in ATTACK_MODES:
            raise ValueError(f"attack must be one of {ATTACK_MODES}, got {attack!r}")
        if block not in BLOCK_MODES:
            raise ValueError(f"block must be one of {BLOCK_MODES}, got {block!r}")
        self.attack = attack
        self.block = block

    def _choose(self, obs, mask) -> int:
        g = np.asarray(obs["globals"]).reshape(-1)
        ridx = int(np.argmax(g[DECISION_ONEHOT_OFF:DECISION_ONEHOT_OFF + NUM_REQUESTS]))
        if ridx == R_DECLARE_ATTACKERS:
            return self._decide_attack(obs, mask)
        if ridx == R_DECLARE_BLOCKERS:
            return self._decide_block(obs, mask)
        return super()._choose(obs, mask)            # land>spell>pass, fallback — unchanged

    # ── attack ─────────────────────────────────────────────────────────────────────────────────────
    def _decide_attack(self, obs, mask) -> int:
        legal = np.flatnonzero(np.asarray(mask, dtype=bool))
        if not mask[COMMIT]:                          # defender sub-step: assign the pending attacker
            return int(legal[0])                      # (one defender in the M1 pool) → first legal
        if self.attack == "never":
            return COMMIT
        perm = [s for s in legal if PERM_BASE <= s < PERM_BASE + MAX_PERM]  # eligible attackers
        if not perm:
            return COMMIT
        if self.attack == "all":
            return int(min(perm))                     # declare next attacker; the loop declares all
        bf = np.asarray(obs["bf_feat"])               # conservative
        blockers = self._enemy_untapped(bf)
        safe = [s for s in perm if self._attack_safe(bf[s - PERM_BASE], blockers)]
        return int(min(safe)) if safe else COMMIT

    @staticmethod
    def _enemy_untapped(bf) -> np.ndarray:
        rows = ((bf[:, BF_PRESENT] > 0.5) & (bf[:, BF_MINE] < 0.5)
                & (bf[:, BF_TAPPED] < 0.5) & (bf[:, BF_TOUGHNESS] > 0))
        return bf[rows][:, [BF_POWER, BF_TOUGHNESS]]  # (k, 2)

    @staticmethod
    def _attack_safe(row, blockers) -> bool:
        if blockers.size == 0:
            return True
        p, t = row[BF_POWER], row[BF_TOUGHNESS]
        loses = (blockers[:, 0] >= t) & (blockers[:, 1] > p)  # enemy kills me and survives
        return not bool(np.any(loses))

    # ── block ──────────────────────────────────────────────────────────────────────────────────────
    def _decide_block(self, obs, mask) -> int:
        legal = np.flatnonzero(np.asarray(mask, dtype=bool))
        if self.block == "never":
            return COMMIT if mask[COMMIT] else int(legal[0])
        bf = np.asarray(obs["bf_feat"])
        if not mask[COMMIT]:                          # attacker sub-step: assign THIS blocker
            enemy = [s for s in legal if PERM_BASE <= s < PERM_BASE + MAX_PERM]
            if not enemy:
                return int(legal[0])
            if self.block == "all":
                return int(min(enemy))                # block the first available attacker
            return self._gang_target(bf, enemy)       # biggest under-blocked attacker
        mine = [s for s in legal if PERM_BASE <= s < PERM_BASE + MAX_PERM]  # blocker-pick step
        if not mine:
            return COMMIT
        if self.block == "all":
            return int(min(mine))                     # throw a blocker at everything
        return int(min(mine)) if self._gang_wants_more(bf) else COMMIT     # gang

    @staticmethod
    def _gang_target(bf, enemy_slots) -> int:
        rows = [(s, bf[s - PERM_BASE, BF_POWER], bf[s - PERM_BASE, BF_TOUGHNESS],
                 bf[s - PERM_BASE, BF_BLOCKED_BY]) for s in enemy_slots]
        under = [r for r in rows if r[3] < GANG_MIN]  # attackers still short of a gang
        pool = under if under else rows
        return int(max(pool, key=lambda r: (r[1], r[2]))[0])   # biggest by power, then toughness

    @staticmethod
    def _gang_wants_more(bf) -> bool:
        atk = (bf[:, BF_PRESENT] > 0.5) & (bf[:, BF_MINE] < 0.5) & (bf[:, BF_ATTACKING] > 0.5)
        return bool(atk.any() and (bf[atk, BF_BLOCKED_BY] < GANG_MIN).any())
