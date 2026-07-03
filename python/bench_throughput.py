"""Repeatable self-play training throughput benchmark — the M3 before/after scorecard.

Pinned config (bears, eval/pool/replay callbacks off) so fps reflects the train loop (env pump +
batched inference), not periodic eval. Reports SB3 steady-state ``time/fps`` + wall time at
n_envs {32,128,256} (override with --n-envs), and writes a dated JSON (config + results + git SHA +
host) to python/bench/ so M3.4's before/after is one command on each side.

    PYTHONPATH=python python/.venv/bin/python python/bench_throughput.py            # 32/128/256
    PYTHONPATH=python python/.venv/bin/python python/bench_throughput.py --n-envs 512 --label m3-after

Baseline (this architecture, BatchedSelfPlayVecEnv, idle GPU): ~1.5-1.65k fps, ≈flat across n_envs —
the wall is the single-threaded Python env pump (serial per-decision engine round-trip), NOT the GPU
(util ~13-25%) or CPU capacity (~3.6/32 cores). See python/bench/throughput_*.json.
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import shutil
import socket
import statistics
import subprocess
import time
from datetime import datetime, timezone

N_STEPS = 256          # PPO n_steps (per env per rollout) — matches train_selfplay
BENCH_DIR = os.path.join(os.path.dirname(__file__), "bench")


def _git_sha() -> str:
    try:
        return subprocess.check_output(["git", "rev-parse", "--short", "HEAD"],
                                       cwd=os.path.dirname(__file__), text=True,
                                       stderr=subprocess.DEVNULL).strip()
    except Exception:
        return "unknown"


def _steady_fps(tb_dir: str) -> tuple[float, list[int]]:
    from tensorboard.backend.event_processing.event_accumulator import EventAccumulator

    subs = sorted(glob.glob(os.path.join(tb_dir, "**", "events.out.tfevents.*"), recursive=True))
    ea = EventAccumulator(os.path.dirname(subs[-1]), size_guidance={"scalars": 0})
    ea.Reload()
    fps = [x.value for x in ea.Scalars("time/fps")]
    steady = statistics.mean(fps[-2:]) if len(fps) >= 2 else (fps[-1] if fps else float("nan"))
    return steady, [round(f) for f in fps]


def bench_one(deck: str, n_envs: int, rollouts: int, seed: int = 0,
              vecenv: str = "batched", num_workers: int = 8) -> dict:
    """One config: run `rollouts` PPO rollouts of pure self-play training, return steady fps + wall."""
    from selfplay_train import train_selfplay

    tb = f"/tmp/mtgenv_benchtb/{deck}_{n_envs}"
    pool = f"/tmp/mtgenv_benchpool/{deck}_{n_envs}/pool"
    for p in (tb, os.path.dirname(pool)):
        shutil.rmtree(p, ignore_errors=True)
    timesteps = N_STEPS * n_envs * rollouts
    t0 = time.perf_counter()
    train_selfplay(deck=deck, timesteps=timesteps, n_envs=n_envs, pool_dir=pool, tensorboard_log=tb,
                   seed=seed, pool_every=10**9, eval_every=10**9, replay_every=0,
                   vecenv=vecenv, num_workers=num_workers,
                   run_name=f"bench-{deck}-{n_envs}",
                   notes=f"throughput bench: {deck}, n_envs={n_envs}, {rollouts} rollouts, vecenv={vecenv}.")
    wall = time.perf_counter() - t0
    steady, series = _steady_fps(tb)
    return {"n_envs": n_envs, "timesteps": timesteps, "rollouts": rollouts,
            "wall_s": round(wall, 1), "steady_fps": round(steady), "fps_series": series}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="bears")
    ap.add_argument("--n-envs", type=int, nargs="*", default=[32, 128, 256],
                    help="one or more n_envs to sweep (default 32 128 256)")
    ap.add_argument("--rollouts", type=int, default=4, help="PPO rollouts per config (steady state)")
    ap.add_argument("--label", default="baseline", help="tag for this bench (e.g. m3-before / m3-after)")
    ap.add_argument("--vecenv", default="batched", choices=["batched", "fleet"],
                    help="'batched' = single-threaded Python pump; 'fleet' = worker-thread parallel stepping")
    ap.add_argument("--num-workers", type=int, default=8, help="fleet worker threads")
    args = ap.parse_args()

    print(f"throughput bench: deck={args.deck} n_envs={args.n_envs} rollouts={args.rollouts} "
          f"vecenv={args.vecenv} workers={args.num_workers} label={args.label}")
    results = []
    for n in args.n_envs:
        r = bench_one(args.deck, n, args.rollouts, vecenv=args.vecenv, num_workers=args.num_workers)
        results.append(r)
        print(f"  n_envs={n:4d}: {r['steady_fps']:6d} fps  wall={r['wall_s']:6.1f}s  series={r['fps_series']}", flush=True)

    base = results[0]["steady_fps"]
    print("\n=== scaling ===")
    for r in results:
        print(f"  n_envs={r['n_envs']:4d}: {r['steady_fps']:6d} fps  ({r['steady_fps']/base:.2f}x vs n_envs={results[0]['n_envs']})")

    os.makedirs(BENCH_DIR, exist_ok=True)
    stamp = datetime.now(timezone.utc).strftime("%Y%m%d-%H%M%S")
    payload = {
        "label": args.label,
        "date_utc": datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC"),
        "git_sha": _git_sha(),
        "host": socket.gethostname(),
        "config": {"deck": args.deck, "n_steps": N_STEPS, "rollouts": args.rollouts,
                   "vecenv": args.vecenv, "num_workers": args.num_workers,
                   "callbacks": "eval/pool/replay OFF"},
        "results": results,
    }
    out = os.path.join(BENCH_DIR, f"throughput_{args.label}_{stamp}.json")
    with open(out, "w") as f:
        json.dump(payload, f, indent=2)
    print(f"\nwrote {out}")


if __name__ == "__main__":
    main()
