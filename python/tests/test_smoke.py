"""Milestone-0 smoke test (GYM_PLAN §7 / §8.0 exit criteria).

Random self-play through the full stack (PyGame bridge → codec → engine thread) must:
  * run many legal games to termination with **no rules panics** (a panic surfaces as a Python
    exception across the FFI seam),
  * present a **non-empty action mask at every decision**,
  * conserve **cards and zones** (object count constant; every card in exactly one zone).

Kept to a few hundred games per deck so it's fast under pytest; the thousands-of-games throughput
run lives in ``python/benchmark.py``.
"""

import numpy as np
import pytest

from mtgenv_gym import MtgEnv, play_random_game, random_action

DECKS = ["lands", "demo", "burn_vs_bears"]
GAMES_PER_DECK = 200


@pytest.mark.parametrize("deck", DECKS)
@pytest.mark.parametrize("auto_pass", [True, False])
def test_random_self_play_is_legal_and_conserves(deck, auto_pass):
    rng = np.random.default_rng(0xC0FFEE ^ hash((deck, auto_pass)) & 0xFFFFFFFF)
    total_decisions = 0
    for seed in range(GAMES_PER_DECK):
        stats = play_random_game(deck=deck, seed=seed, auto_pass=auto_pass, rng=rng)
        total_decisions += stats.decisions
        assert stats.decisions > 0, f"{deck}/{seed}: a real game has decisions"
        assert stats.min_legal >= 1, f"{deck}/{seed}: empty mask surfaced"
        assert stats.cards_conserved, (
            f"{deck}/{seed}: card conservation "
            f"({stats.object_count} != {stats.initial_object_count})"
        )
        assert stats.zones_conserved, (
            f"{deck}/{seed}: zone conservation "
            f"({stats.zone_sum} != {stats.object_count})"
        )
    # Sanity: self-play actually exercises a non-trivial number of decisions.
    assert total_decisions > GAMES_PER_DECK, f"{deck}: suspiciously few decisions"


def test_seed_is_deterministic():
    # Same seed + same (deterministic) RNG stream ⇒ identical game outcome and shape.
    a = play_random_game(deck="demo", seed=42, rng=np.random.default_rng(7))
    b = play_random_game(deck="demo", seed=42, rng=np.random.default_rng(7))
    assert (a.decisions, a.winner, a.turns, a.reason) == (b.decisions, b.winner, b.turns, b.reason)


def test_mtg_env_gym_api_runs_episodes():
    env = MtgEnv(deck="demo", auto_pass=True)
    # Spaces are read from the extension, not hard-coded (representation-swappable seam).
    assert env.observation_space.shape == (env.obs_dim,)
    assert env.action_space.n == env.action_dim

    rng = np.random.default_rng(1)
    for ep in range(25):
        obs, info = env.reset(seed=ep)
        assert obs.shape == (env.obs_dim,)
        assert np.isfinite(obs).all()
        steps = 0
        terminated = truncated = False
        while not (terminated or truncated):
            mask = info["action_mask"]
            assert mask.sum() >= 1, "non-empty mask every step"
            action = random_action(mask, rng)
            obs, reward, terminated, truncated, info = env.step(action)
            assert np.isfinite(obs).all()
            steps += 1
        assert terminated or truncated
        if terminated:
            s = info["summary"]
            assert s["object_count"] == s["initial_object_count"], "cards conserved"
            assert s["zone_sum"] == s["object_count"], "zones conserved"
            assert reward in (-1.0, 0.0, 1.0)
