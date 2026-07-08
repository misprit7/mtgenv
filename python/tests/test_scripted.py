"""ScriptedPolicy (python/mtgenv_gym/evalkit/scripted.py) — the land>spell>attack-all>never-block
reference. Synthetic-obs branch coverage of the decision priority, a codec/obs-layout pin (so a
Discrete(98) or globals-offset drift is caught), and a real-heralds integration drive asserting it
plays lands, attacks, never blocks, and never picks an illegal action.
"""

import numpy as np

from mtgenv_gym.evalkit import ScriptedPolicy
from mtgenv_gym.evalkit.scripted import (
    COMMIT,
    DECISION_ONEHOT_OFF,
    HAND_BASE,
    HAND_LAND_COL,
    NO,
    PERM_BASE,
    R_DECLARE_ATTACKERS,
    R_DECLARE_BLOCKERS,
    R_PRIORITY,
    YES,
)

_POL = ScriptedPolicy()


def _obs(ridx, hand_lands=(), hand_spells=()):
    """Minimal obs: a decision-kind one-hot in globals + a hand_feat table with land/spell rows."""
    g = np.zeros(69, dtype=np.float32)
    g[DECISION_ONEHOT_OFF + ridx] = 1.0
    hand = np.zeros((16, 18), dtype=np.float32)
    for r in list(hand_lands) + list(hand_spells):
        hand[r, 0] = 1.0                          # present
    for r in hand_lands:
        hand[r, HAND_LAND_COL] = 1.0              # land-type flag
    return {"globals": g, "hand_feat": hand}


def _mask(slots):
    m = np.zeros(98, dtype=bool)
    m[list(slots)] = True
    return m


def _act(obs, mask):
    return int(_POL.act([obs], [mask])[0])


# ── decision-priority branches ───────────────────────────────────────────────────────────────────
def test_priority_plays_land_over_spell_over_pass():
    # hand row 0 = land (slot 1), row 1 = spell (slot 2); both legal + pass legal → land wins.
    obs = _obs(R_PRIORITY, hand_lands=[0], hand_spells=[1])
    assert _act(obs, _mask([COMMIT, HAND_BASE, HAND_BASE + 1])) == HAND_BASE       # play the land
    # only the spell legal → cast it.
    assert _act(_obs(R_PRIORITY, hand_spells=[1]), _mask([COMMIT, HAND_BASE + 1])) == HAND_BASE + 1
    # nothing playable (only pass) → pass.
    assert _act(_obs(R_PRIORITY), _mask([COMMIT])) == COMMIT
    # first (lowest-index) land wins the tie: rows 2 and 0 both land → slot 1 (< slot 3).
    obs2 = _obs(R_PRIORITY, hand_lands=[0, 2])
    assert _act(obs2, _mask([COMMIT, HAND_BASE, HAND_BASE + 2])) == HAND_BASE


def test_attack_with_everything_then_commit():
    # attacker toggles legal → take one (not commit yet); lowest-index first.
    obs = _obs(R_DECLARE_ATTACKERS)
    assert _act(obs, _mask([COMMIT, PERM_BASE, PERM_BASE + 1])) == PERM_BASE
    # only commit legal (all attackers declared) → commit.
    assert _act(obs, _mask([COMMIT])) == COMMIT


def test_never_blocks():
    obs = _obs(R_DECLARE_BLOCKERS)
    # a blocker toggle is legal but the script commits with no blocks.
    assert _act(obs, _mask([COMMIT, PERM_BASE, PERM_BASE + 1])) == COMMIT


def test_fallback_prefers_commit_then_decline():
    # a generic decision with COMMIT legal → commit.
    assert _act(_obs(4), _mask([COMMIT, YES, NO])) == COMMIT
    # mulligan (no COMMIT): keep = NO, never mulligan-to-death (YES is the lower index but NO wins).
    assert _act(_obs(1), _mask([YES, NO])) == NO
    # no commit / no NO → first legal slot.
    assert _act(_obs(6), _mask([PERM_BASE + 3, PERM_BASE + 5])) == PERM_BASE + 3


# ── codec / obs layout pin ───────────────────────────────────────────────────────────────────────
def test_layout_matches_engine():
    import mtg_py

    assert mtg_py.PyGame.action_dim() == 98
    spec = dict((name, cols) for (name, rows, cols, is_int) in mtg_py.PyGame.obs_spec())
    assert spec["globals"] == 69          # decision one-hot at [43,64) relies on this width
    assert spec["hand_feat"] == 18        # HAND_LAND_COL indexes into this row


# ── real heralds drive ───────────────────────────────────────────────────────────────────────────
def test_scripted_on_real_heralds():
    from mtgenv_gym.env import MtgEnv

    env = MtgEnv(deck="heralds", opponent="external")
    saw_land = saw_attack = False
    for seed in range(4):                 # a few games so land+attack both surface
        rng = np.random.default_rng(1000 + seed)
        env.ext_reset(seed)
        for _ in range(4000):
            st = env.ext_state()
            if st == "terminal":
                break
            obs, mask = env.ext_obs(), env.ext_mask()
            if st == "learner":
                a = _act(obs, mask)
                assert mask[a], f"scripted chose illegal action {a}"
                ridx = int(np.argmax(np.asarray(obs["globals"])[DECISION_ONEHOT_OFF:DECISION_ONEHOT_OFF + 21]))
                if ridx == R_DECLARE_BLOCKERS:
                    assert a == COMMIT, "scripted must never block"
                if ridx == R_DECLARE_ATTACKERS and a != COMMIT:
                    saw_attack = True
                if ridx == R_PRIORITY and HAND_BASE <= a < HAND_BASE + 16 \
                        and obs["hand_feat"][a - HAND_BASE, HAND_LAND_COL] > 0.5:
                    saw_land = True
                env.ext_apply(a)
            else:
                env.ext_apply(int(rng.choice(np.flatnonzero(mask))))
        if saw_land and saw_attack:
            break
    assert saw_land, "scripted never played a land on heralds"
    assert saw_attack, "scripted never attacked on heralds"
