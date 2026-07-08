"""SwineBlockAnalyzer combat-judgment metrics (python/mtgenv_gym/evalkit/analyzers.py) on constructed
states: chump_block_rate (lone-bear block of the swine, not-forced; forced kept separate),
double_block_rate (2+ gang on the swine), lone_bear_attack_rate (one bear into an untapped enemy swine).
"""

import numpy as np

from mtgenv_gym.evalkit.analyzers import SwineBlockAnalyzer

# obs layout (obs.rs; must match analyzers.py)
_G_LIFE, _G_DEC0 = 16, 43
_R_ATK, _R_BLK = 9, 10
COL = dict(present=0, mine=1, power=2, tapped=6, attacking=39, blocked_by=43, pending=44)
F_PERM = 45


def _obs(ridx, rows, my_life=20):
    """rows: list of dicts of bf_feat column-name -> value. Builds globals (decision one-hot + life)."""
    g = np.zeros(69, dtype=np.float32)
    g[_G_DEC0 + ridx] = 1.0
    g[_G_LIFE] = my_life
    bf = np.zeros((32, F_PERM), dtype=np.float32)
    for i, r in enumerate(rows):
        bf[i, COL["present"]] = 1.0
        for k, v in r.items():
            bf[i, COL[k]] = v
    return {"globals": g, "bf_feat": bf}


def _bear(mine, **kw):
    return {"mine": 1.0 if mine else 0.0, "power": 2.0, **kw}


def _swine(mine, **kw):
    return {"mine": 1.0 if mine else 0.0, "power": 3.0, **kw}


# ── blocking ─────────────────────────────────────────────────────────────────────────────────────
def test_lone_bear_chump_not_forced():
    a = SwineBlockAnalyzer()
    # Enemy swine attacking (blocked_by=1), my bear pending-blocking it. Life 20 → not forced.
    obs = _obs(_R_BLK, [
        _swine(False, attacking=1.0, blocked_by=1.0),   # the attacking swine, lone-blocked
        _bear(True, pending=1.0),                        # my lone-bear blocker
    ], my_life=20)
    a.observe(obs, {"block_eligible": 1, "block_declared": 1, "block_double": 0, "attackers_blocked": 1})
    r = a.result()
    assert r["swine/n_swine_attacks"] == 1.0
    assert r["swine/chump_block_rate"] == 1.0          # a chump, not forced
    assert r["swine/double_block_rate"] == 0.0
    assert r["swine/n_forced_swine_attacks"] == 0.0


def test_lone_bear_chump_forced_is_bucketed_separately():
    a = SwineBlockAnalyzer()
    # Life 3, swine power 3 → not blocking is lethal → forced. Chump is defensible; must NOT count.
    obs = _obs(_R_BLK, [_swine(False, attacking=1.0, blocked_by=1.0), _bear(True, pending=1.0)], my_life=3)
    a.observe(obs, {"block_eligible": 1, "block_declared": 1, "block_double": 0, "attackers_blocked": 1})
    r = a.result()
    assert r["swine/n_forced_swine_attacks"] == 1.0
    assert np.isnan(r["swine/chump_block_rate"])       # no NOT-forced swine-attacks
    assert r["swine/chump_block_rate_forced"] == 1.0


def test_double_block_is_correct_line():
    a = SwineBlockAnalyzer()
    # Two of my bears gang the swine (blocked_by=2). Correct play → double_block_rate 1, chump 0.
    obs = _obs(_R_BLK, [
        _swine(False, attacking=1.0, blocked_by=2.0),
        _bear(True, pending=1.0),
        _bear(True, pending=1.0),
    ], my_life=20)
    a.observe(obs, {"block_eligible": 2, "block_declared": 2, "block_double": 1, "attackers_blocked": 1})
    r = a.result()
    assert r["swine/double_block_rate"] == 1.0
    assert r["swine/chump_block_rate"] == 0.0


def test_no_swine_attack_not_recorded():
    a = SwineBlockAnalyzer()
    # Only an enemy BEAR attacking → not a swine-attack; nothing recorded.
    obs = _obs(_R_BLK, [_bear(False, attacking=1.0, blocked_by=1.0), _bear(True, pending=1.0)])
    a.observe(obs, {"block_eligible": 1, "block_declared": 1, "block_double": 0, "attackers_blocked": 1})
    assert a.result()["swine/n_swine_attacks"] == 0.0


# ── attacking ────────────────────────────────────────────────────────────────────────────────────
def test_lone_bear_attack_into_untapped_swine():
    a = SwineBlockAnalyzer()
    # I declare exactly one bear (pending); enemy has an untapped swine that could block → bad play.
    obs = _obs(_R_ATK, [_bear(True, pending=1.0), _swine(False, tapped=0.0)])
    a.observe(obs, {"attack_eligible": 2, "attack_declared": 1})
    r = a.result()
    assert r["swine/n_attack_decisions"] == 1.0
    assert r["swine/lone_bear_attack_rate"] == 1.0


def test_lone_swine_attack_is_not_lone_bear():
    a = SwineBlockAnalyzer()
    # Attacking with a single SWINE (not a bear) into an untapped enemy swine → not the flagged play.
    obs = _obs(_R_ATK, [_swine(True, pending=1.0), _swine(False, tapped=0.0)])
    a.observe(obs, {"attack_eligible": 2, "attack_declared": 1})
    assert a.result()["swine/lone_bear_attack_rate"] == 0.0


def test_attack_not_scored_when_enemy_swine_tapped():
    a = SwineBlockAnalyzer()
    # Enemy swine is tapped → cannot block → this attack decision is not scored at all.
    obs = _obs(_R_ATK, [_bear(True, pending=1.0), _swine(False, tapped=1.0)])
    a.observe(obs, {"attack_eligible": 1, "attack_declared": 1})
    assert a.result()["swine/n_attack_decisions"] == 0.0
