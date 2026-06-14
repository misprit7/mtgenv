"""M2 self-play seam tests: MtgEnv accepts a *policy* opponent (callable / object), gets a
perspective-correct obs for it, and resamples it per episode. (The frozen-checkpoint pool is
exercised in the learning test; here we just pin the seam with a trivial scripted opponent.)
"""

import numpy as np

from mtgenv_gym import MtgEnv


class _FirstLegalOpponent:
    """A trivial scripted opponent: take the first legal action. Tracks that it gets a real obs."""

    def __init__(self):
        self.calls = 0
        self.reset_calls = 0

    def reset(self, rng):
        self.reset_calls += 1

    def act(self, obs, mask):
        self.calls += 1
        # The obs handed to the opponent must be the structured Dict (its own perspective).
        assert isinstance(obs, dict) and "globals" in obs and np.isfinite(obs["globals"]).all()
        return int(np.flatnonzero(mask)[0])


def test_policy_opponent_seam_runs_and_is_resampled():
    opp = _FirstLegalOpponent()
    env = MtgEnv(deck="demo", opponent=opp)
    rng = np.random.default_rng(0)
    for ep in range(8):
        obs, info = env.reset(seed=ep)
        done = False
        while not done:
            mask = info["action_mask"]
            assert mask.sum() >= 1
            assert info["seat"] == env.agent_seat, "only agent decisions surface"
            obs, r, term, trunc, info = env.step(int(np.flatnonzero(mask)[0]))
            done = term or trunc
        if term:
            assert r in (-1.0, 0.0, 1.0)
    assert opp.reset_calls == 8, "opponent resampled once per episode"
    assert opp.calls > 0, "opponent actually answered its seat's decisions with a real obs"


def test_callable_opponent_also_works():
    rng = np.random.default_rng(1)
    env = MtgEnv(deck="demo", opponent=lambda obs, mask: int(rng.choice(np.flatnonzero(mask))))
    obs, info = env.reset(seed=5)
    done = False
    while not done:
        obs, r, term, trunc, info = env.step(int(np.flatnonzero(info["action_mask"])[0]))
        done = term or trunc
    assert term or trunc
