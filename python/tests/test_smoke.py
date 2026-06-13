"""Milestone-1 smoke test: the factored action codec + structured obs run legal random self-play
to termination (no panics, non-empty mask at every sub-step, card/zone conservation), and the Gym
``MtgEnv`` produces well-formed Dict observations + masks and terminal rewards.

Kept fast (a few hundred games); the throughput run is ``python/benchmark.py`` and the learning
sanity check is ``tests/test_learning.py``.
"""

import numpy as np
import pytest

from mtgenv_gym import MtgEnv, play_random_game, random_action

DECKS = ["lands", "demo", "burn_vs_bears"]
GAMES_PER_DECK = 150


@pytest.mark.parametrize("deck", DECKS)
@pytest.mark.parametrize("auto_pass", [True, False])
def test_random_self_play_is_legal_and_conserves(deck, auto_pass):
    rng = np.random.default_rng(0xC0FFEE ^ (hash((deck, auto_pass)) & 0xFFFFFFFF))
    total_decisions = 0
    for seed in range(GAMES_PER_DECK):
        stats = play_random_game(deck=deck, seed=seed, auto_pass=auto_pass, rng=rng)
        total_decisions += stats.decisions
        assert stats.decisions > 0, f"{deck}/{seed}: a real game has sub-step decisions"
        assert stats.min_legal >= 1, f"{deck}/{seed}: empty mask surfaced"
        assert stats.cards_conserved, f"{deck}/{seed}: cards not conserved"
        assert stats.zones_conserved, f"{deck}/{seed}: zones not conserved"
    assert total_decisions > GAMES_PER_DECK


def test_seed_is_deterministic():
    a = play_random_game(deck="demo", seed=42, rng=np.random.default_rng(7))
    b = play_random_game(deck="demo", seed=42, rng=np.random.default_rng(7))
    assert (a.decisions, a.winner, a.turns, a.reason) == (b.decisions, b.winner, b.turns, b.reason)


def test_mtg_env_dict_obs_and_masks():
    import gymnasium as gym

    env = MtgEnv(deck="demo", auto_pass=True)
    assert isinstance(env.observation_space, gym.spaces.Dict)
    assert env.action_space.n == env.action_dim
    # The space is built from the extension's obs_spec — keys must match what step returns.
    expected_keys = {name for (name, *_rest) in env._spec}
    assert set(env.observation_space.spaces) == expected_keys

    rng = np.random.default_rng(1)
    rewards = []
    for ep in range(20):
        obs, info = env.reset(seed=ep)
        _check_obs(env, obs)
        terminated = truncated = False
        steps = 0
        reward = 0.0
        while not (terminated or truncated):
            mask = info["action_mask"]
            assert mask.sum() >= 1, "non-empty mask every step"
            assert info["seat"] == env.agent_seat, "only the agent's decisions are surfaced"
            obs, reward, terminated, truncated, info = env.step(random_action(mask, rng))
            _check_obs(env, obs)
            steps += 1
        rewards.append(reward)
        if terminated:
            s = info["summary"]
            assert s["object_count"] == s["initial_object_count"], "cards conserved"
            assert s["zone_sum"] == s["object_count"], "zones conserved"
            assert reward in (-1.0, 0.0, 1.0)
    # A random agent vs random opponent should both win and lose some games (reward varies).
    assert any(r > 0 for r in rewards) or any(r < 0 for r in rewards)


def _check_obs(env, obs):
    assert set(obs) == set(env.observation_space.spaces)
    for name, space in env.observation_space.spaces.items():
        assert obs[name].shape == space.shape, f"{name}: {obs[name].shape} != {space.shape}"
        assert np.isfinite(obs[name]).all(), f"{name}: non-finite"
