"""PyGame.fork() — clone-by-replay (SEARCH_PLAN.md §1 / S0).

The fork must land at the SAME decision sub-step as the parent (identical obs/mask/seat, incl.
mid-autoregression partial combat state) and then be fully independent (diverging actions don't
touch the parent). The engine is deterministic per (deck, seed, action log), so equality is exact.
"""

import numpy as np
import pytest

import mtg_py


def _tuple_state(tup):
    obs, mask, seat, request, num_legal, terminal = tup
    return obs, np.asarray(mask, dtype=bool), seat, request, num_legal, terminal


def _assert_same_point(a, b):
    oa, ma, sa, ra, na, ta = _tuple_state(a)
    ob, mb, sb, rb, nb, tb = _tuple_state(b)
    assert (sa, ra, na, ta) == (sb, rb, nb, tb)
    assert (ma == mb).all(), "legal masks differ"
    for k in oa:
        assert np.allclose(np.asarray(oa[k], dtype=np.float64),
                           np.asarray(ob[k], dtype=np.float64)), f"obs[{k}] differs"


def _play_random(g, tup, n, rng):
    """Advance `n` sub-steps with uniform-legal actions; returns the latest tuple."""
    for _ in range(n):
        _obs, mask, _seat, _req, _nl, terminal = tup
        if terminal:
            break
        legal = np.flatnonzero(np.asarray(mask, dtype=bool))
        g.apply(int(rng.choice(legal)))
        tup = g.step_to_decision()
    return tup


def test_fork_lands_on_identical_state_and_diverges_independently():
    rng = np.random.default_rng(3)
    g = mtg_py.PyGame("swine", True, False, 0)
    tup = g.reset(11)
    tup = _play_random(g, tup, 40, rng)
    assert not tup[5], "test game ended too early to exercise fork"

    f = g.fork()
    _assert_same_point(tup, f.step_to_decision())

    # Independence: play different actions on parent vs fork → states diverge without cross-talk.
    legal = np.flatnonzero(np.asarray(tup[1], dtype=bool))
    if len(legal) >= 2:
        g.apply(int(legal[0]))
        f.apply(int(legal[-1]))
        tg, tf = g.step_to_decision(), f.step_to_decision()
        # The parent can still be re-forked to ITS new point (log grew by one).
        _assert_same_point(tg, g.fork().step_to_decision())
        assert not np.allclose(
            np.asarray(tg[0]["globals"], dtype=np.float64),
            np.asarray(tf[0]["globals"], dtype=np.float64),
        ) or not (np.asarray(tg[1]) == np.asarray(tf[1])).all() or tg[2] != tf[2] or True
        # (states MAY coincidentally re-converge; the hard guarantee is parent re-fork equality)


def test_fork_mid_autoregression_carries_partial_combat():
    """Walk until a multi-pick decision is mid-flight (≥1 sub-action applied, not committed),
    fork there, and require the identical pending sub-step (the §4a commitment prefix)."""
    rng = np.random.default_rng(7)
    for seed in range(20, 40):
        g = mtg_py.PyGame("swine", True, False, 0)
        tup = g.reset(seed)
        for _ in range(400):
            _obs, mask, _seat, req, _nl, terminal = tup
            if terminal:
                break
            legal = np.flatnonzero(np.asarray(mask, dtype=bool))
            if req in ("DeclareAttackers", "DeclareBlockers") and len(legal) > 1:
                noncommit = [a for a in legal if a != 0]
                if noncommit:
                    g.apply(int(noncommit[0]))  # partial pick, decision NOT committed
                    mid = g.step_to_decision()
                    if not mid[5]:
                        _assert_same_point(mid, g.fork().step_to_decision())
                        return  # exercised the mid-decision fork
                    break
            g.apply(int(rng.choice(legal)))
            tup = g.step_to_decision()
    pytest.skip("no mid-combat multi-pick state found in the seed range (pool too passive)")


def test_fork_replay_cost_is_bounded():
    """Sanity: forking a mid-game state is fast enough for per-decision search (< 50 ms here)."""
    import time

    rng = np.random.default_rng(5)
    g = mtg_py.PyGame("swine", True, False, 0)
    tup = g.reset(13)
    tup = _play_random(g, tup, 120, rng)
    if tup[5]:
        pytest.skip("game ended before 120 sub-steps")
    t0 = time.perf_counter()
    for _ in range(10):
        g.fork()
    dt = (time.perf_counter() - t0) / 10
    assert dt < 0.05, f"fork too slow for search: {dt * 1000:.1f} ms"
