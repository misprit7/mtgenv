#!/usr/bin/env python3
"""Training-loop profiling harness (measurement only — no behaviour change to the real trainers).

Answers "what is actually expensive in the PPO self-play loop, and what does the eval battery cost?"
by running the SAME stack (fleet vec env + MaskablePPO + EntityExtractor) under a TOGGLE MATRIX and
reporting steps/s + GPU util for each config, plus a stripped ceiling. It's also the py-spy target:

    # sampling profile of the full-battery loop → folded stacks (aggregate with --report)
    python/.venv/bin/py-spy record --native --rate 120 --duration 45 --format raw -o data/prof/full.folded -- \
        python/.venv/bin/python scripts/profile_train.py --config full --steps 300000
    python/.venv/bin/python scripts/profile_train.py --fold data/prof/full.folded   # top-N attribution

    # toggle matrix (steps/s per config)
    python/.venv/bin/python scripts/profile_train.py --matrix --steps 60000

Configs (one variable off at a time vs `full`): ladder_off, opp_off (vs_random only), eval_25k,
replay_off, stripped (no EvalkitCallback at all — the raw-PPO ceiling).
"""

from __future__ import annotations

import argparse
import glob
import os
import subprocess
import threading
import time

DECK = "swine"
N_ENVS = 8
POOL = "/tmp/mtgenv_profile_pool"
TB = "/tmp/mtgenv_profile_tb"

CONFIGS = ["full", "ladder_off", "opp_off", "eval_25k", "replay_off", "stripped"]


class GPUSampler(threading.Thread):
    """Poll nvidia-smi GPU utilization every 0.5s; keep mean/max over the run."""

    def __init__(self):
        super().__init__(daemon=True)
        self._stop = threading.Event()
        self.samples = []

    def run(self):
        while not self._stop.is_set():
            try:
                out = subprocess.check_output(
                    ["nvidia-smi", "--query-gpu=utilization.gpu", "--format=csv,noheader,nounits"],
                    text=True, stderr=subprocess.DEVNULL, timeout=2)
                self.samples.append(int(out.strip().splitlines()[0]))
            except Exception:
                pass
            self._stop.wait(0.5)

    def stop(self):
        self._stop.set()

    def stats(self):
        s = self.samples
        return (sum(s) / len(s) if s else float("nan"), max(s) if s else float("nan"))


def build(config: str, steps: int):
    """A fresh model + callback list for `config`, mirroring selfplay_train's stack."""
    from sb3_contrib import MaskablePPO

    from mtgenv_gym.policy import EntityExtractor
    from mtgenv_gym.tracked_stats import TrackedStatsCallback
    from mtgenv_gym.tb_meta import GameLengthCallback
    from selfplay_train import make_vecenv

    os.makedirs(POOL, exist_ok=True)
    for f in glob.glob(os.path.join(POOL, "*.zip")):
        os.remove(f)
    venv = make_vecenv(DECK, POOL, N_ENVS, seed=0, vecenv="fleet", num_workers=8)
    model = MaskablePPO("MultiInputPolicy", venv,
                        policy_kwargs=dict(features_extractor_class=EntityExtractor),
                        n_steps=256, batch_size=256, gamma=0.999, ent_coef=0.01, seed=0,
                        tensorboard_log=TB, verbose=0)
    # seed the opponent pool + a vs-initial reference so the opponent forwards + evals are realistic.
    ref = os.path.join(POOL, "..", "profile_ref.zip")
    model.save(ref[:-4])
    model.save(os.path.join(POOL, "ckpt_000000000"))

    cbs = [TrackedStatsCallback(), GameLengthCallback()]  # cheap, always on (read the rollout stream)
    if config != "stripped":
        from mtgenv_gym.evalkit import EvalkitCallback

        cbs.append(EvalkitCallback(
            DECK, total_timesteps=steps, n_envs=N_ENVS,
            eval_freq=(25_000 if config == "eval_25k" else 8_000),
            ref_path=("/nonexistent.zip" if config == "opp_off" else ref),
            ladder_dir=POOL + "_ladder", n_games=40,
            milestones=(() if config == "ladder_off" else (0.10, 0.25, 0.50, 0.75)),
            replay_every=(0 if config == "replay_off" else 25_000),
            eval_script=(config != "opp_off"), run_name="profile", verbose=0))
    return model, cbs


def run_config(config: str, steps: int) -> dict:
    model, cbs = build(config, steps)
    gpu = GPUSampler()
    gpu.start()
    t0 = time.time()
    model.learn(total_timesteps=steps, callback=cbs, progress_bar=False)
    wall = time.time() - t0
    gpu.stop()
    model.env.close()
    g_mean, g_max = gpu.stats()
    return dict(config=config, steps=steps, wall=wall, sps=steps / wall, gpu_mean=g_mean, gpu_max=g_max)


def matrix(steps: int):
    print(f"{'config':<12} {'steps/s':>9} {'wall(s)':>8} {'gpu%mean':>9} {'gpu%max':>8}  {'vs full':>8}")
    results = []
    for c in CONFIGS:
        r = run_config(c, steps)
        results.append(r)
        print(f"{r['config']:<12} {r['sps']:>9.0f} {r['wall']:>8.1f} {r['gpu_mean']:>9.1f} "
              f"{r['gpu_max']:>8.0f}", flush=True)
    full = next(r for r in results if r["config"] == "full")
    print("\n=== speedup vs full (steps/s ratio) ===")
    for r in results:
        print(f"  {r['config']:<12} {r['sps']/full['sps']:>5.2f}x  ({r['sps']:.0f} sps)")
    strip = next(r for r in results if r["config"] == "stripped")
    print(f"\nfull→stripped callback overhead: {strip['sps']/full['sps']:.2f}x "
          f"({full['sps']:.0f} → {strip['sps']:.0f} sps); stripped ceiling {strip['sps']:.0f} steps/s.")


def fold_report(path: str, top: int = 30):
    """Aggregate py-spy raw folded stacks: self-time (leaf frame) by (file:func), top-N by wall %."""
    leaf, total = {}, 0
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            stack, _, cnt = line.rpartition(" ")
            try:
                cnt = int(cnt)
            except ValueError:
                continue
            frames = stack.split(";")
            if not frames:
                continue
            leaf[frames[-1]] = leaf.get(frames[-1], 0) + cnt
            total += cnt
    print(f"top {top} self-time frames ({total} samples):")
    for frame, cnt in sorted(leaf.items(), key=lambda x: -x[1])[:top]:
        print(f"  {100*cnt/total:5.1f}%  {frame[:110]}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--config", choices=CONFIGS, default="full")
    ap.add_argument("--steps", type=int, default=60_000)
    ap.add_argument("--matrix", action="store_true", help="run the whole toggle matrix")
    ap.add_argument("--fold", metavar="FOLDED", help="aggregate a py-spy raw folded-stacks file")
    args = ap.parse_args()
    if args.fold:
        fold_report(args.fold)
    elif args.matrix:
        matrix(args.steps)
    else:
        r = run_config(args.config, args.steps)  # single run (the py-spy target)
        print(f"{r['config']}: {r['sps']:.0f} steps/s, wall {r['wall']:.1f}s, gpu {r['gpu_mean']:.0f}%")


if __name__ == "__main__":
    main()
