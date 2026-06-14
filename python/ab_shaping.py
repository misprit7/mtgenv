"""A/B: does potential-based reward shaping (GYM_PLAN §5) speed up self-play learning?

Trains shaped (coef0>0, annealed) vs unshaped self-play for a short budget across seeds and reports
win-rate **vs random** (the unbiased metric — shaping only affects the *training* reward, eval uses
the true ±1). Shaping is a learning crutch, so the question is "faster/stabler at a fixed budget",
not "higher asymptote". On the simple demo deck both saturate fast, so a small/short A/B may land
within noise; the payoff is expected to be larger on the richer M4 pool / longer games.

    PYTHONPATH=python python/.venv/bin/python python/ab_shaping.py --deck demo --timesteps 40000 --seeds 3
"""
from __future__ import annotations

import argparse
import glob
import os
import statistics
import time

from selfplay_train import play_winrate, train_selfplay


def one(deck, timesteps, n_envs, seed, coef):
    pool = f"/tmp/mtgenv_ab_{int(coef * 100)}_{seed}"
    os.makedirs(pool, exist_ok=True)
    for f in glob.glob(pool + "/*.zip"):
        os.remove(f)
    model, _ref = train_selfplay(
        deck=deck, timesteps=timesteps, n_envs=n_envs, pool_dir=pool, seed=seed,
        shaping_coef=coef, pool_every=max(timesteps // 8, 4000), eval_every=10**9,  # eval at end only
    )
    return play_winrate(model, deck, "random", 300, 9_000_000 + seed)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="demo")
    ap.add_argument("--timesteps", type=int, default=40_000)
    ap.add_argument("--n-envs", type=int, default=32)
    ap.add_argument("--seeds", type=int, default=3)
    args = ap.parse_args()
    print(f"A/B shaping: deck={args.deck} timesteps={args.timesteps} seeds={args.seeds} (vs-random win-rate)")
    summary = {}
    for coef in (0.0, 0.5):
        wrs = []
        for seed in range(args.seeds):
            t0 = time.time()
            wr = one(args.deck, args.timesteps, args.n_envs, seed, coef)
            wrs.append(wr)
            print(f"  coef={coef} seed={seed}: vs_random={wr:.3f}  ({time.time() - t0:.0f}s)", flush=True)
        m = statistics.mean(wrs)
        summary[coef] = m
        print(f"coef={coef}: mean vs_random={m:.3f}  (n={len(wrs)})", flush=True)
    d = summary[0.5] - summary[0.0]
    print(f"\nshaped − unshaped = {d:+.3f}  → {'shaping helps' if d > 0.02 else 'within noise' if abs(d) <= 0.02 else 'shaping hurts'}")


if __name__ == "__main__":
    main()
