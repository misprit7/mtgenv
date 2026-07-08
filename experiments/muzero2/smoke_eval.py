"""Dev-only: validate the mz_policy evalkit adapter end-to-end on a --smoke checkpoint.

The smoke trainer shrinks the shape-determining knobs (latent/head/lstm/chance/embed/unroll), so the
eval model must be built with the SAME shrunk values to load the checkpoint. This passes them via
``build_policy(cfg_overrides=...)`` and runs a few evalkit games so the adapter's greedy+sampled ``act``
and the canonical TB tags are exercised for each algo. Real watchers use the mz_policy CLI (defaults
match the trained model) and do not need this.

Run:  PYTHONPATH=../../python .venv/bin/python smoke_eval.py --algo efficientzero --ckpt <ckpt> --step 14
"""
from __future__ import annotations

import argparse

import torch

from mz_policy import MuZeroLzPolicy, build_policy


# The shrink knobs the --smoke trainer uses (train.py SMOKE branch) — the eval build must match.
SMOKE_OVERRIDES = dict(
    latent_state_dim=64, head_hidden=(32,), num_simulations=8,
    lstm_hidden_size=64, chance_space_size=4,
    embed_dim=64, num_layers=2, num_heads=2, infer_context_length=4,
    num_unroll_steps=5,
)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--algo", required=True,
                    choices=["gumbel", "muzero", "efficientzero", "stochastic_muzero", "unizero"])
    ap.add_argument("--deck", default="heralds")
    ap.add_argument("--ckpt", required=True)
    ap.add_argument("--step", type=int, default=0)
    ap.add_argument("--games", type=int, default=4)
    ap.add_argument("--run-dir", default="/tmp/mtgenv_tb/smoke-eval")
    ap.add_argument("--device", default="cuda" if torch.cuda.is_available() else "cpu")
    args = ap.parse_args()

    from mtgenv_gym.evalkit import evaluate_checkpoint
    ov = dict(SMOKE_OVERRIDES)
    if args.algo == "unizero":
        ov.update(collector_env_num=1, evaluator_env_num=1)
    policy = build_policy(args.algo, args.deck, args.ckpt, device=args.device,
                          latent=ov["latent_state_dim"], sims=ov["num_simulations"],
                          head_hidden=ov["head_hidden"], cfg_overrides=ov)
    adapter = MuZeroLzPolicy(policy, device=args.device, temp=0.25, algo=args.algo)
    is_uz = (args.algo == "unizero")
    res = evaluate_checkpoint(adapter, step=args.step, run_dir=args.run_dir, deck=args.deck,
                              games=args.games, run_name=f"smoke-{args.algo}", algo=f"{args.algo}-mz",
                              batch_size=(1 if is_uz else 64), record_replay=not is_uz)
    g = res["selfplay/winrate_vs_random"]["greedy"]
    s = res["selfplay/winrate_vs_random"]["sample"]
    print(f"[smoke_eval OK] {args.algo}: greedy={g.win_rate:.3f} sampled={s.win_rate:.3f} "
          f"turns={g.avg_turns:.1f} prod={g.stats.get('productive_rate', float('nan')):.2f} "
          f"n={g.n_games}", flush=True)


if __name__ == "__main__":
    main()
