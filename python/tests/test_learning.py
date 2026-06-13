"""Milestone-1 learning-sanity test (GYM_PLAN §8.1 exit): MaskablePPO trained on the env beats a
random opponent — the signal that obs + action mask + reward are wired correctly.

Uses ``burn_vs_bears`` because random seat-0 (burn) wins only ~5% there, so even a short train run
produces a large, low-variance margin. Marked ``slow`` (~20s); run the full training via
``python/train.py`` for headline numbers.
"""

import pytest

pytest.importorskip("numpy")
pytest.importorskip("sb3_contrib")

from train import train_and_eval  # noqa: E402  (after importorskip)


@pytest.mark.slow
def test_maskable_ppo_beats_random():
    res = train_and_eval(deck="burn_vs_bears", timesteps=12_000, eval_games=120, n_envs=8, seed=0)
    base = res["baseline"]["win_rate"]
    trained = res["trained"]["win_rate"]
    assert trained > base + 0.10, f"trained {trained:.3f} did not beat baseline {base:.3f}"
    assert trained >= 0.15, f"trained win-rate {trained:.3f} implausibly low"
