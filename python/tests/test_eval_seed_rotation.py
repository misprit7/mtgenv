"""Rotating eval-seed contract (tb_logging.eval_seed): consecutive evals must sample DIFFERENT game
seeds so a converged policy's curve stops being an exactly-flat frozen-test-set line. Covers the pure
helper + the evaluate_checkpoint path (the resolved `seed` recorded per EvalResult must change with step
and stay distinct per opponent).
"""

import tempfile

from mtgenv_gym.evalkit import RandomPolicy
from mtgenv_gym.evalkit.hooks import evaluate_checkpoint
from mtgenv_gym.evalkit.tb_logging import eval_seed


def test_eval_seed_rotates_and_is_reproducible():
    base = 5_000_000
    assert eval_seed(base, 0) == base
    assert eval_seed(base, None) == base
    # consecutive evals (step jumps by eval_freq) get distinct, non-overlapping seed ranges.
    steps = [0, 8000, 16000, 24000]
    seeds = [eval_seed(base, s) for s in steps]
    assert len(set(seeds)) == len(seeds), "eval seeds must differ across steps"
    assert seeds == sorted(seeds) and all(b - a >= 40 for a, b in zip(seeds, seeds[1:])), \
        "gaps ≥ n_games so [seed, seed+n_games) ranges never overlap"
    # deterministic: same (base, step) reproduces the same seed (JSON stays reproducible).
    assert eval_seed(base, 12345) == eval_seed(base, 12345)


def test_consecutive_evaluate_checkpoint_use_different_seeds():
    pol = RandomPolicy(1)
    opponents = {"selfplay/winrate_vs_random": RandomPolicy(2),
                 "selfplay/winrate_vs_script": RandomPolicy(3)}
    with tempfile.TemporaryDirectory() as d:
        r1 = evaluate_checkpoint(pol, step=8000, run_dir=d, deck="heralds", opponents=opponents,
                                 games=6, record_replay=False, modes=("greedy",))
        r2 = evaluate_checkpoint(pol, step=16000, run_dir=d, deck="heralds", opponents=opponents,
                                 games=6, record_replay=False, modes=("greedy",))
    for tag in opponents:
        s1 = r1[tag]["greedy"].seed
        s2 = r2[tag]["greedy"].seed
        assert s1 != s2, f"{tag}: eval seed did not rotate across steps ({s1} == {s2})"
    # and the two opponents used DIFFERENT seeds within the same eval (distinct per-opponent bases).
    assert (r1["selfplay/winrate_vs_random"]["greedy"].seed
            != r1["selfplay/winrate_vs_script"]["greedy"].seed), "opponents must not share a seed"
