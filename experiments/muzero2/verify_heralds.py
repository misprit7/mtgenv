"""HERALDS VERIFICATION — airtight, large-N, fresh-seed re-eval of the peak checkpoint.

Independent of the training watcher: 500 greedy + 500 sampled games vs RandomPolicy on NEW seeds (base
7_000_000, the watcher used 5_000_000), via evalkit. Eval is inherently shaping-free — the Arena drives
MtgEnv(opponent="external") and reads win/loss from ext_reward() (raw ±1); `reward_shaping` is a param of
the MtgLzEnv TRAINING wrapper only and never touches eval. Records several replays for the web UI.

Run: PYTHONPATH=../../python .venv/bin/python verify_heralds.py \
        --ckpt tb/3.5-muzero-heralds/ckpt/iteration_7000.pth.tar --games 500 --seed 7000000
"""
from __future__ import annotations

import argparse
import hashlib
import os

import numpy as np

from mz_policy import build_policy, MuZeroLzPolicy
from mtgenv_gym.evalkit import Arena, RandomPolicy, record_game
from mtgenv_gym.evalkit.replay import REPLAY_DIR
from mtgenv_gym.evalkit.tb_logging import WriterRecorder, log_eval, write_json


def md5(path):
    h = hashlib.md5()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ckpt", required=True)
    ap.add_argument("--deck", default="heralds")
    ap.add_argument("--games", type=int, default=500)
    ap.add_argument("--sims", type=int, default=50)
    ap.add_argument("--latent", type=int, default=256)
    ap.add_argument("--seed", type=int, default=7_000_000, help="FRESH base seed (watcher used 5,000,000)")
    ap.add_argument("--opp-seed", type=int, default=7_000_000, help="RandomPolicy opponent seed")
    ap.add_argument("--replays", type=int, default=6)
    ap.add_argument("--run-dir", default="/tmp/mtgenv_tb/3.5-heralds-verify")
    ap.add_argument("--device", default="cuda")
    args = ap.parse_args()

    print("=" * 78)
    print("HERALDS VERIFICATION")
    print(f"  checkpoint : {os.path.abspath(args.ckpt)}")
    print(f"  md5        : {md5(args.ckpt)}")
    print(f"  deck       : {args.deck}   opponent: RandomPolicy(seed={args.opp_seed})  (== MtgEnv opponent='random')")
    print(f"  eval config: algo=muzero  latent={args.latent}  sims={args.sims}  games={args.games} per mode")
    print(f"  seeds      : FRESH base {args.seed} (watcher used 5,000,000 — no overlap)")
    print(f"  shaping    : OFF at eval by construction — Arena reads raw ±1 ext_reward(); "
          f"reward_shaping is a training-wrapper param only")
    print("=" * 78, flush=True)

    policy = build_policy("muzero", args.deck, args.ckpt, device=args.device, latent=args.latent, sims=args.sims)
    adapter = MuZeroLzPolicy(policy, device=args.device, algo="muzero")
    opp = RandomPolicy(seed=args.opp_seed)

    arena = Arena(args.deck, batch_size=64)
    results = arena.evaluate(adapter, opp, n_games=args.games, seed=args.seed,
                             opponent_label="selfplay/winrate_vs_random", modes=("greedy", "sample"))

    for mode, r in results.items():
        lo, hi = r.win_ci95
        print(f"\n[{mode.upper()}]  win_rate = {r.win_rate:.3f}   (95% Wilson {lo:.3f}–{hi:.3f})   "
              f"n={r.n_games}  W/L/D={r.wins}/{r.losses}/{r.draws}")
        print(f"          turns_mean = {r.avg_turns:.2f}")
        print(f"          end_reasons = " + ", ".join(f"{k}={v:.3f}" for k, v in sorted(r.end_reasons.items())))
        if r.stats:
            print(f"          stats = " + ", ".join(f"{k}={v:.2f}" for k, v in sorted(r.stats.items())))

    # canonical TB tags + JSON into the verify run dir
    from torch.utils.tensorboard import SummaryWriter
    writer = SummaryWriter(args.run_dir)
    rec = WriterRecorder(writer)
    step = 120113  # env-step of iteration_7000
    log_eval(rec, results, win_tag="selfplay/winrate_vs_random", step=step,
             with_stats=True, with_game=True, with_analyzers=True)
    labelled = {f"winrate_vs_random_{m}": r for m, r in results.items()}
    jpath = write_json(args.run_dir, step, labelled)
    writer.flush(); writer.close()
    print(f"\nTB tags + JSON written to {args.run_dir}  (json: {os.path.basename(jpath)})")

    # several replays for the web UI (distinct seeds -> distinct games)
    print(f"\nrecording {args.replays} replays to {REPLAY_DIR} (web lobby 'AI Training Replays') ...")
    paths = []
    for k in range(args.replays):
        p = record_game(adapter, args.deck, step + k, run_name="3.5-heralds-verify", algo="MuZero",
                        seed=args.seed + 1000 + k)
        paths.append(os.path.basename(p) if p else "—")
    print("  replays: " + ", ".join(paths))
    print("\nDONE. Numbers only — no interpretation. Compare greedy win_rate above to the 0.93 claim.")


if __name__ == "__main__":
    main()
