"""ScriptedHeuristic — the parameterized scripted benchmark family (attack × block axes).

Synthetic-obs branch coverage of each axis off the bf_feat board (power/toughness/tapped/attacking/
blocked_by columns) + the autoregressive combat sub-steps (mask[COMMIT] separates the top-level pick
from the defender/which-attacker sub-pick), a racer==ScriptedPolicy equivalence pin, and a real-swine
integration drive asserting every variant plays only legal actions and honors its never-attack /
never-block invariant.
"""

import numpy as np

from mtgenv_gym.evalkit.scripted import (
    ACTION_DIM,
    BF_ATTACKING,
    BF_BLOCKED_BY,
    BF_MINE,
    BF_POWER,
    BF_PRESENT,
    BF_TAPPED,
    BF_TOUGHNESS,
    COMMIT,
    DECISION_ONEHOT_OFF,
    HAND_BASE,
    HAND_LAND_COL,
    MAX_PERM,
    NO,
    NUM_REQUESTS,
    PERM_BASE,
    PLAYER_BASE,
    R_DECLARE_ATTACKERS,
    R_DECLARE_BLOCKERS,
    R_PRIORITY,
    ScriptedHeuristic,
    ScriptedPolicy,
    YES,
)

def _bf_width():
    """Synthetic bf_feat width = the live engine's (v2=48, v3=45, …); fallback 48. The policy only
    reads columns ≤ BF_BLOCKED_BY=43, so any width ≥44 is correct — this just matches the contract."""
    try:
        import mtg_py

        return {n: c for (n, _r, c, _i) in mtg_py.PyGame.obs_spec()}["bf_feat"]
    except Exception:
        return 48


_BF_W = _bf_width()
_G_W = 69
_VARIANTS = {  # name -> (attack, block); mirrors rate_agent.SCRIPT_KINDS
    "racer": ("all", "never"), "turtle": ("never", "all"),
    "gang": ("all", "gang"), "careful": ("conservative", "gang"),
}


def _bf(creatures):
    bf = np.zeros((MAX_PERM, _BF_W), np.float32)
    for c in creatures:
        r = c["row"]
        bf[r, BF_PRESENT] = 1.0
        bf[r, BF_MINE] = 1.0 if c["mine"] else 0.0
        bf[r, BF_POWER] = c["power"]
        bf[r, BF_TOUGHNESS] = c["tough"]
        bf[r, BF_TAPPED] = c.get("tapped", 0)
        bf[r, BF_ATTACKING] = c.get("attacking", 0)
        bf[r, BF_BLOCKED_BY] = c.get("blocked_by", 0)
    return bf


def _obs(ridx, creatures=(), hand_lands=(), hand_spells=()):
    g = np.zeros(_G_W, np.float32)
    g[DECISION_ONEHOT_OFF + ridx] = 1.0
    hand = np.zeros((16, 18), np.float32)
    for r in list(hand_lands) + list(hand_spells):
        hand[r, 0] = 1.0
    for r in hand_lands:
        hand[r, HAND_LAND_COL] = 1.0
    return {"globals": g, "bf_feat": _bf(creatures), "hand_feat": hand}


def _mask(slots):
    m = np.zeros(ACTION_DIM, bool)     # sized by the live contract (v1=98, v2=130, …)
    m[list(slots)] = True
    return m


def _act(pol, obs, mask):
    return int(pol.act([obs], [mask])[0])


# ── attack axis ──────────────────────────────────────────────────────────────────────────────────
def test_attack_never_commits():
    pol = ScriptedHeuristic(attack="never", block="never")
    obs = _obs(R_DECLARE_ATTACKERS, [{"row": 0, "mine": True, "power": 2, "tough": 2}])
    assert _act(pol, obs, _mask([COMMIT, PERM_BASE])) == COMMIT


def test_attack_all_takes_lowest_then_commits():
    pol = ScriptedHeuristic(attack="all", block="never")
    obs = _obs(R_DECLARE_ATTACKERS, [{"row": 0, "mine": True, "power": 2, "tough": 2},
                                     {"row": 1, "mine": True, "power": 2, "tough": 2}])
    assert _act(pol, obs, _mask([COMMIT, PERM_BASE, PERM_BASE + 1])) == PERM_BASE
    assert _act(pol, obs, _mask([COMMIT])) == COMMIT


def test_attack_defender_substep_picks_first_legal():
    pol = ScriptedHeuristic(attack="all", block="never")
    obs = _obs(R_DECLARE_ATTACKERS, [{"row": 0, "mine": True, "power": 2, "tough": 2}])
    assert _act(pol, obs, _mask([PLAYER_BASE, PLAYER_BASE + 1])) == PLAYER_BASE  # no COMMIT ⇒ defender


def test_attack_conservative_skips_into_bigger_blocker():
    pol = ScriptedHeuristic(attack="conservative", block="gang")
    obs = _obs(R_DECLARE_ATTACKERS, [{"row": 0, "mine": True, "power": 2, "tough": 2},
                                     {"row": 1, "mine": False, "power": 3, "tough": 3}])  # 3/3 untapped
    assert _act(pol, obs, _mask([COMMIT, PERM_BASE])) == COMMIT  # losing attack ⇒ hold


def test_attack_conservative_attacks_when_safe():
    pol = ScriptedHeuristic(attack="conservative", block="gang")
    obs = _obs(R_DECLARE_ATTACKERS, [{"row": 0, "mine": True, "power": 2, "tough": 2},
                                     {"row": 1, "mine": False, "power": 1, "tough": 1}])  # can't kill me
    assert _act(pol, obs, _mask([COMMIT, PERM_BASE])) == PERM_BASE


def test_attack_conservative_takes_even_trade_and_ignores_tapped():
    # even trade (2/2 vs 2/2) is allowed; a bigger blocker that is TAPPED is not a threat.
    pol = ScriptedHeuristic(attack="conservative", block="gang")
    even = _obs(R_DECLARE_ATTACKERS, [{"row": 0, "mine": True, "power": 2, "tough": 2},
                                      {"row": 1, "mine": False, "power": 2, "tough": 2}])
    assert _act(pol, even, _mask([COMMIT, PERM_BASE])) == PERM_BASE
    tapped = _obs(R_DECLARE_ATTACKERS, [{"row": 0, "mine": True, "power": 2, "tough": 2},
                                        {"row": 1, "mine": False, "power": 3, "tough": 3, "tapped": 1}])
    assert _act(pol, tapped, _mask([COMMIT, PERM_BASE])) == PERM_BASE


# ── block axis ───────────────────────────────────────────────────────────────────────────────────
def test_block_never_commits():
    pol = ScriptedHeuristic(attack="all", block="never")
    obs = _obs(R_DECLARE_BLOCKERS, [{"row": 0, "mine": True, "power": 2, "tough": 2},
                                    {"row": 5, "mine": False, "power": 3, "tough": 3, "attacking": 1}])
    assert _act(pol, obs, _mask([COMMIT, PERM_BASE])) == COMMIT


def test_block_all_assigns_blocker_then_attacker():
    pol = ScriptedHeuristic(attack="never", block="all")
    obs = _obs(R_DECLARE_BLOCKERS, [{"row": 0, "mine": True, "power": 2, "tough": 2},
                                    {"row": 5, "mine": False, "power": 3, "tough": 3, "attacking": 1}])
    assert _act(pol, obs, _mask([COMMIT, PERM_BASE])) == PERM_BASE          # pick a blocker
    assert _act(pol, obs, _mask([PERM_BASE + 5])) == PERM_BASE + 5          # sub-step: block it


def test_block_gang_targets_biggest_underblocked():
    pol = ScriptedHeuristic(attack="all", block="gang")
    obs = _obs(R_DECLARE_BLOCKERS, [
        {"row": 0, "mine": True, "power": 2, "tough": 2},
        {"row": 5, "mine": False, "power": 3, "tough": 3, "attacking": 1, "blocked_by": 0},  # big
        {"row": 6, "mine": False, "power": 1, "tough": 1, "attacking": 1, "blocked_by": 0},  # small
    ])
    assert _act(pol, obs, _mask([COMMIT, PERM_BASE])) == PERM_BASE          # wants more ⇒ pick blocker
    assert _act(pol, obs, _mask([PERM_BASE + 5, PERM_BASE + 6])) == PERM_BASE + 5   # ⇒ biggest


def test_block_gang_commits_when_all_ganged():
    pol = ScriptedHeuristic(attack="all", block="gang")
    obs = _obs(R_DECLARE_BLOCKERS, [
        {"row": 0, "mine": True, "power": 2, "tough": 2},                    # a spare blocker
        {"row": 5, "mine": False, "power": 3, "tough": 3, "attacking": 1, "blocked_by": 2},
    ])
    assert _act(pol, obs, _mask([COMMIT, PERM_BASE])) == COMMIT              # already ganged ⇒ stop


# ── racer == the pinned ScriptedPolicy ─────────────────────────────────────────────────────────────
def test_racer_matches_scripted_policy():
    racer, base = ScriptedHeuristic(attack="all", block="never"), ScriptedPolicy()
    scenarios = [
        (_obs(R_PRIORITY, hand_lands=[0], hand_spells=[1]), _mask([COMMIT, HAND_BASE, HAND_BASE + 1])),
        (_obs(R_DECLARE_ATTACKERS, [{"row": 0, "mine": True, "power": 2, "tough": 2}]),
         _mask([COMMIT, PERM_BASE])),
        (_obs(R_DECLARE_ATTACKERS), _mask([PLAYER_BASE])),                  # defender sub-step
        (_obs(R_DECLARE_BLOCKERS, [{"row": 0, "mine": True, "power": 2, "tough": 2}]),
         _mask([COMMIT, PERM_BASE])),
        (_obs(4), _mask([COMMIT, YES, NO])),                                # generic → commit
        (_obs(1), _mask([YES, NO])),                                        # mulligan → keep (NO)
    ]
    for obs, mask in scenarios:
        assert _act(racer, obs, mask) == _act(base, obs, mask)


def test_bad_axis_rejected():
    import pytest
    with pytest.raises(ValueError):
        ScriptedHeuristic(attack="turbo", block="never")
    with pytest.raises(ValueError):
        ScriptedHeuristic(attack="all", block="sometimes")


# ── real-swine integration drive (each variant: legal + honors its invariant) ──────────────────────
def test_variants_play_legally_on_swine():
    from mtgenv_gym.env import MtgEnv

    for name, (atk, blk) in _VARIANTS.items():
        pol = ScriptedHeuristic(attack=atk, block=blk)
        env = MtgEnv(deck="swine", opponent="external")
        for seed in range(2):
            rng = np.random.default_rng(500 + seed)
            env.ext_reset(seed)
            for _ in range(4000):
                st = env.ext_state()
                if st == "terminal":
                    break
                obs, mask = env.ext_obs(), env.ext_mask()
                if st == "learner":
                    a = _act(pol, obs, mask)
                    assert mask[a], f"{name} chose illegal action {a}"
                    g = np.asarray(obs["globals"])
                    ridx = int(np.argmax(g[DECISION_ONEHOT_OFF:DECISION_ONEHOT_OFF + NUM_REQUESTS]))
                    if atk == "never" and ridx == R_DECLARE_ATTACKERS and mask[COMMIT]:
                        assert a == COMMIT, f"{name} declared an attacker"
                    if blk == "never" and ridx == R_DECLARE_BLOCKERS:
                        assert a == COMMIT, f"{name} declared a block"
                    env.ext_apply(a)
                else:
                    env.ext_apply(int(rng.choice(np.flatnonzero(mask))))
