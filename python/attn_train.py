"""Relational-attention + pointer-head PPO trainer — the relational-encoding arm (4.7-4.9).

Same self-play regime, vec env, reward shaping, evalkit battery (vs_random / vs_script / ladder /
stats / game / swine analyzers / replays) and CLI discipline as ``selfplay_train.py`` — it just swaps
the policy to ``RelationalPointerPolicy`` (attention encoder over entity+relation graph + content-based
pointer head), so 4.7-4.9 overlay the 4.4-4.6 mean-pool+indexed baselines on one dashboard.

    PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 PYTHONPATH=python python/.venv/bin/python python/attn_train.py \
        --deck swine --timesteps 500000 --run-name 4.9-ppo-attn-swine --notes "..."
"""

from __future__ import annotations

import argparse

from mtgenv_gym.attn_policy import RelationalPointerPolicy
from selfplay_train import play_winrate, train_selfplay
from mtgenv_gym.league import ModelOpponent


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="swine",
                    choices=["lands", "demo", "burn_vs_bears", "selesnya", "heralds", "bears", "swine"])
    ap.add_argument("--timesteps", type=int, default=500_000)
    ap.add_argument("--n-envs", type=int, default=8)
    ap.add_argument("--pool-dir", default="/tmp/mtgenv_pool_attn")
    ap.add_argument("--tensorboard", default="/home/xander/dev/p-mtg/mtgenv/data/tb")
    ap.add_argument("--shaping-coef", type=float, default=0.1)
    ap.add_argument("--vecenv", default="fleet", choices=["fleet", "batched"])
    ap.add_argument("--num-workers", type=int, default=8)
    ap.add_argument("--replay-every", type=int, default=25_000)
    ap.add_argument("--eval-every", type=int, default=8000, help="steps between the evalkit battery")
    ap.add_argument("--pool-every", type=int, default=8000, help="steps between self-play pool snapshots")
    ap.add_argument("--run-name", required=True, help="exact TB run name, e.g. 4.7-ppo-attn-swine")
    # PARITY defaults (~138k params ≈ baseline ~142k). d_model=256/ff=512 is the parked size experiment.
    ap.add_argument("--d-model", type=int, default=48)
    ap.add_argument("--ff", type=int, default=128)
    ap.add_argument("--layers", type=int, default=2)
    ap.add_argument("--notes", required=True,
                    help="REQUIRED: what this run tests → TB 'run/notes'. State the relational hypothesis.")
    args = ap.parse_args()

    model, ref = train_selfplay(
        deck=args.deck, timesteps=args.timesteps, n_envs=args.n_envs, pool_dir=args.pool_dir,
        tensorboard_log=args.tensorboard, shaping_coef=args.shaping_coef, notes=args.notes,
        replay_every=args.replay_every, run_name=args.run_name, vecenv=args.vecenv,
        num_workers=args.num_workers, eval_every=args.eval_every, pool_every=args.pool_every, verbose=1,
        policy=RelationalPointerPolicy,
        policy_kwargs=dict(d_model=args.d_model, ff=args.ff, layers=args.layers),
    )
    wr_rand = play_winrate(model, args.deck, "random", 200, 9_000_000)
    wr_init = play_winrate(model, args.deck, ModelOpponent(ref), 200, 9_500_000)
    print(f"\nfinal: win-rate vs random {wr_rand:.3f} | vs initial self {wr_init:.3f}")


if __name__ == "__main__":
    main()
