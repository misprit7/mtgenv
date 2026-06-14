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


@pytest.mark.slow
def test_selfplay_improves_over_initial(tmp_path):
    """M2 exit signal: a self-play league policy beats its own initial (random-init) checkpoint and
    stays strong vs random — on the demo deck (mirror)."""
    from selfplay_train import play_winrate, train_selfplay
    from mtgenv_gym.league import ModelOpponent

    model, ref = train_selfplay(
        deck="demo", timesteps=16_000, n_envs=8, pool_dir=str(tmp_path / "pool"),
        pool_every=4000, eval_every=10**9, seed=0,  # skip in-training eval; final-eval below
    )
    wr_init = play_winrate(model, "demo", ModelOpponent(ref), 120, 7_000_000)
    wr_rand = play_winrate(model, "demo", "random", 120, 8_000_000)
    assert wr_init > 0.55, f"self-play did not beat its initial self ({wr_init:.2f})"
    assert wr_rand > 0.55, f"weak vs random ({wr_rand:.2f})"
