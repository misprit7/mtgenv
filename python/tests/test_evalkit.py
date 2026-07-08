"""evalkit — the algorithm-agnostic eval/metrics/logging framework (python/mtgenv_gym/evalkit).

Fast torch-free coverage of Arena / Ladder / analyzers on tiny budgets (heralds, 20 games), plus SB3
adapter coverage: the migration guarantee that Arena greedy-vs-random is **bit-identical** to the
legacy ``play_winrate`` path, and that ``EvalkitCallback`` trains and emits the canonical tag set.
"""

import glob
import os
import tempfile

import numpy as np

from mtgenv_gym.evalkit import Arena, EvalResult, Ladder, RandomPolicy, wilson_ci
from mtgenv_gym.evalkit.analyzers import get_analyzer

_STAT_KEYS = {"attack_rate", "productive_rate", "block_rate", "cast_rate", "playland_rate",
              "block_double_rate"}


# ── torch-free core ────────────────────────────────────────────────────────────────────────────
def test_wilson_ci_bounds():
    assert wilson_ci(0, 0) == (0.0, 0.0)
    lo, hi = wilson_ci(20, 40)
    assert 0.0 <= lo <= 0.5 <= hi <= 1.0
    lo, hi = wilson_ci(40, 40)          # all wins → upper near 1, lower well below 1
    assert hi == 1.0 and lo < 1.0


def test_arena_random_result_is_sane_and_deterministic():
    ar = Arena("heralds", batch_size=8)
    r1 = ar.play(RandomPolicy(1), RandomPolicy(2), n_games=20, seed=5_000_000, opponent_label="random")
    r2 = ar.play(RandomPolicy(1), RandomPolicy(2), n_games=20, seed=5_000_000, opponent_label="random")
    assert isinstance(r1, EvalResult)
    assert r1.n_games == 20 and r1.wins + r1.losses + r1.draws == 20
    assert 0.0 <= r1.win_rate <= 1.0
    lo, hi = r1.win_ci95
    assert lo <= r1.win_rate <= hi
    assert r1.avg_turns > 0
    assert set(r1.stats) == _STAT_KEYS
    # deterministic given (seed, n_games, batch_size)
    assert r1.win_rate == r2.win_rate and r1.wins == r2.wins
    assert r1.avg_turns == r2.avg_turns


def test_arena_both_modes():
    res = Arena("heralds", batch_size=8).evaluate(
        RandomPolicy(1), RandomPolicy(2), n_games=12, seed=5_000_000, opponent_label="random")
    assert set(res) == {"greedy", "sample"}
    assert res["greedy"].mode == "greedy" and res["sample"].mode == "sample"


def test_swine_analyzer_runs_on_swine_only():
    assert get_analyzer("bears") is None
    a = get_analyzer("swine")
    assert a is not None and a.name == "swine"
    res = Arena("swine", batch_size=8).play(
        RandomPolicy(1), RandomPolicy(2), n_games=16, seed=3_000_000, opponent_label="random")
    assert "swine/n_swine_attacks" in res.analyzers
    assert "swine/double_block_rate" in res.analyzers
    # bears deck (no analyzer) → empty analyzers block
    res_b = Arena("bears", batch_size=8).play(
        RandomPolicy(1), RandomPolicy(2), n_games=8, seed=3_000_000, opponent_label="random")
    assert res_b.analyzers == {}


def test_ladder_snapshots_and_nan_for_unreached():
    ar = Arena("heralds", batch_size=8)
    saved = {}

    def snap(path):
        saved[path] = True
        return path

    lad = Ladder(tempfile.mkdtemp(), snap, lambda p: RandomPolicy(seed=1),
                 milestones=(0.10, 0.25, 0.50, 0.75), n_games=8)
    lad.maybe_snapshot(0.30)   # reaches 10% and 25% only

    class Rec:
        def __init__(self):
            self.d = {}

        def record(self, tag, v, step=None):
            self.d[tag] = v

    rec = Rec()
    tags = lad.eval_and_log(RandomPolicy(9), ar, rec, step=100)
    assert set(tags) == {f"ladder/winrate_vs_{p:02d}pct" for p in (10, 25, 50, 75)}
    assert not np.isnan(tags["ladder/winrate_vs_10pct"])
    assert not np.isnan(tags["ladder/winrate_vs_25pct"])
    assert np.isnan(tags["ladder/winrate_vs_50pct"])   # not reached → gap
    assert np.isnan(tags["ladder/winrate_vs_75pct"])


# ── SB3 adapter ────────────────────────────────────────────────────────────────────────────────
def _tiny_model(deck, pool):
    from sb3_contrib import MaskablePPO

    from mtgenv_gym import BatchedSelfPlayVecEnv
    from mtgenv_gym.policy import EntityExtractor

    ve = BatchedSelfPlayVecEnv(deck, pool, 4, p_random=1.0, seed=0)
    model = MaskablePPO("MultiInputPolicy", ve,
                        policy_kwargs=dict(features_extractor_class=EntityExtractor),
                        n_steps=64, batch_size=64, verbose=0)
    return model, ve


def test_sb3_arena_greedy_vs_random_matches_legacy_play_winrate():
    """The migration guarantee: Arena greedy-vs-random reproduces the legacy per-game
    ``play_winrate`` exactly (same engine seeds + replayed opponent RNG stream)."""
    import pytest

    pytest.importorskip("torch")
    from mtgenv_gym.evalkit import SB3Policy
    from selfplay_train import play_winrate

    pool = tempfile.mkdtemp()
    model, ve = _tiny_model("heralds", pool)
    ve.close()

    legacy = play_winrate(model, "heralds", "random", n_games=20, seed0=5_000_000)
    arena = Arena("heralds", batch_size=20).play(
        SB3Policy(model), RandomPolicy(), n_games=20, seed=5_000_000, a_mode="greedy",
        opponent_label="random")
    assert arena.win_rate == legacy, f"arena {arena.win_rate} != legacy {legacy}"


def test_evalkit_callback_trains_and_emits_canonical_tags():
    import pytest

    pytest.importorskip("torch")

    from mtgenv_gym.evalkit import EvalkitCallback
    from mtgenv_gym.tb_meta import GameLengthCallback
    from mtgenv_gym.tracked_stats import TrackedStatsCallback

    pool = tempfile.mkdtemp()
    model, ve = _tiny_model("heralds", pool)
    model.save(os.path.join(pool, "ckpt_000000000"))
    ref = os.path.join(pool, "ref.zip")
    model.save(ref[:-4])

    tb = tempfile.mkdtemp()
    model.set_logger(_tb_logger(tb))
    cbs = [
        EvalkitCallback("heralds", total_timesteps=512, eval_freq=128, n_envs=4, ref_path=ref,
                        ladder_dir=os.path.join(pool, "ladder"), n_games=6, replay_every=0),
        TrackedStatsCallback(),
        GameLengthCallback(),
    ]
    model.learn(total_timesteps=512, callback=cbs, progress_bar=False)
    ve.close()

    tags = _read_tags(tb)
    # win-rate battery (evalkit) + behaviour stats (TrackedStats) + game length (GameLength)
    assert "selfplay/winrate_vs_random" in tags
    assert "selfplay/winrate_vs_random_sampled" in tags        # the MuZero-lesson addition
    assert "selfplay/winrate_vs_initial" in tags
    assert {f"ladder/winrate_vs_{p:02d}pct" for p in (10, 25, 50, 75)} <= tags
    assert {f"stats/{k}" for k in _STAT_KEYS} <= tags
    assert "game/turns_mean" in tags


# ── tiny TB helpers ──────────────────────────────────────────────────────────────────────────────
def _tb_logger(tb_dir):
    from stable_baselines3.common.logger import configure

    return configure(tb_dir, ["tensorboard"])


def _read_tags(tb_dir):
    from tensorboard.backend.event_processing.event_accumulator import EventAccumulator

    subs = sorted(glob.glob(os.path.join(tb_dir, "**", "events.out.tfevents.*"), recursive=True))
    ea = EventAccumulator(os.path.dirname(subs[-1]), size_guidance={"scalars": 0})
    ea.Reload()
    return set(ea.Tags().get("scalars", []))
