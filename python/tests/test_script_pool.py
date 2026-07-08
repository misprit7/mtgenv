"""Punisher-script opponents in the self-play pool (batched_selfplay._PooledBatchedOpponent p_script).
The hypothesis-fix after 4.7: mirror self-play doesn't punish bad combat; fixed heuristic opponents do.
"""

import numpy as np

from mtgenv_gym.batched_selfplay import _build_script_pool, _PooledBatchedOpponent
from mtgenv_gym.evalkit.scripted import ScriptedHeuristic


def test_build_script_pool_maps_variants():
    pool = _build_script_pool(["gang", "careful", "turtle", "bogus"])
    assert len(pool) == 3 and all(isinstance(p, ScriptedHeuristic) for p in pool)  # bogus skipped


def test_p_script_assigns_heuristics():
    # Empty checkpoint pool + p_random 0 + p_script 1.0 → every assignment is a ScriptedHeuristic.
    opp = _PooledBatchedOpponent("/tmp/nonexistent_pool_xyz", p_random=0.0, rng_seed=0,
                                 p_script=1.0, script_mix=["gang", "careful"])
    for i in range(8):
        opp.assign(i)
    kinds = [opp._assign[i] for i in range(8)]
    assert all(isinstance(k, ScriptedHeuristic) for k in kinds), "p_script=1 must assign heuristics"
    assert len({id(k) for k in kinds}) >= 1  # drawn from the 2-variant pool

    # p_script 0 → never a heuristic (back to random/checkpoint behaviour).
    opp0 = _PooledBatchedOpponent("/tmp/nonexistent_pool_xyz", p_random=1.0, rng_seed=0,
                                  p_script=0.0, script_mix=["gang"])
    opp0.assign(0)
    assert opp0._assign[0] is None


def test_resolve_answers_script_opponent_on_real_env():
    from mtgenv_gym.env import MtgEnv

    opp = _PooledBatchedOpponent("/tmp/nonexistent_pool_xyz", p_random=0.0, rng_seed=1,
                                 p_script=1.0, script_mix=["careful"])
    env = MtgEnv(deck="swine", opponent="external")
    env.ext_reset(0)
    # advance to any decision (learner or opponent — the pool answers whichever is pending here)
    opp.assign(0)
    acts = opp.resolve([(0, env)], np.random.default_rng(0))
    a = acts[0]
    assert env.ext_mask()[a], "script opponent returned an illegal action"
