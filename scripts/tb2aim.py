#!/usr/bin/env python3
"""Mirror TensorBoard runs into an Aim repo (idempotent; optionally watch-loop).

The training stacks (SB3 PPO callbacks, LightZero, the evalkit watcher) all write TB event
files under one logdir (default ``/tmp/mtgenv_tb``). This script mirrors every run dir into a
persistent Aim repo (default ``data/aim``) so Aim is the *view* while TB event files remain the
write format — no training code changes, and runs from any venv (py3.11 muzero2, py3.14 gym)
all land in one place.

Idempotent: per-(run, tag) high-water-mark steps live in ``<repo>/tb2aim_state.json``; re-runs
only import new events. ``--watch N`` re-syncs every N seconds forever (run it detached).

Run with the muzero2 venv (has aim + tensorboard):
    experiments/muzero2/.venv/bin/python scripts/tb2aim.py [--watch 120]
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time

from aim import Run
from tensorboard.backend.event_processing.event_accumulator import EventAccumulator

# ── grouping + freeform context (what TB can't express) ────────────────────────────────────────
# Aim "experiment" = section in the UI. Rule-based so future runs auto-classify.
def experiment_for(name: str) -> str:
    if name.startswith("smoke") or name.startswith("2.6-probe"):
        return "smoke-and-probes"
    if name.startswith("2."):
        return "ppo-selfplay"
    if name[:3] in ("3.0", "3.1", "3.2", "3.3"):
        return "muzero-v1-failed"
    if name.endswith("-train"):
        return "muzero2-train-internals"
    if name in ("tmp-heralds-peak", "3.5-heralds-verify") or "verify" in name:
        return "verification"
    if name.startswith("3."):
        return "muzero2"
    return "misc"


DESCRIPTIONS = {
    "3.5-muzero-heralds": (
        "THE heralds winner. Plain MuZero (LightZero), latent 256, sims 50, td_steps 40, "
        "unroll 5, update_per_collect 20, reanalyze 0.25, random_collect 32, constant PBRS "
        "shaping 0.5 (train only — eval return is raw ±1). Peak ckpt iteration_7000 "
        "(env-step 120113): greedy 0.93 / sampled 0.92 vs random, then drifts to ~0.65. "
        "Recipe log: experiments/muzero2/3.5-muzero-heralds.log"
    ),
    "3.5-heralds-verify": (
        "Independent fresh-seed verification of the 3.5 peak ckpt (seed 7,000,000, 500 games/mode, "
        "verify_heralds.py): greedy 0.926 (Wilson CI 0.900–0.946), sampled 0.924. Shaping off by "
        "construction. Replays in web lobby 'AI Training Replays'."
    ),
    "tmp-heralds-peak": (
        "Original 200-game re-eval of the 3.5 peak ckpt (seed 5,000,000): greedy 0.93 / sampled 0.92. "
        "Superseded by 3.5-heralds-verify (500 games, fresh seed)."
    ),
    "3.4-gumbel-heralds": (
        "Gumbel-MuZero A/B arm — FAILED (~0.04 at gate, killed). Fragile low-sim completed-Q; "
        "see muzero2/README.md."
    ),
    "3.5-muzero-reanalyze-heralds": "5-minute false start; superseded by 3.5-muzero-heralds.",
    "3.6-muzero-swine": "First swine attempt, crashed ~32k steps.",
    "3.6-muzero-swine-r2": "Swine restart — STOPPED per user directive pending heralds verification.",
    "2.9-swine-500k_1": (
        "PPO swine baseline (evalkit watcher schema). Known failure: chump-blocks 94–97% at life≥15 "
        "(swine/chump_rate_swine_hi)."
    ),
}


def event_dirs(path: str) -> "list[str]":
    """All dirs under ``path`` (inclusive, following symlinks) that directly hold event files.
    SB3 nests its events one level down (``<run>/MaskablePPO_1/``); LightZero under ``log/serial``."""
    found = []
    for d, _, files in os.walk(path, followlinks=True):
        if any(f.startswith("events.out.tfevents") and not f.endswith(".notes") for f in files):
            found.append(d)
    return sorted(found)


def sync_run(tb_root: str, name: str, repo: str, state: dict) -> int:
    """Sync one top-level TB run dir (all nested event dirs fold into ONE Aim run)."""
    path = os.path.join(tb_root, name)
    st = state.setdefault(name, {"hash": None, "tags": {}})
    run = None  # open lazily — only if there is something new to write
    imported = 0
    try:
        for d in event_dirs(path):
            rel = os.path.relpath(d, path)
            acc = EventAccumulator(d, size_guidance={"scalars": 0})
            try:
                acc.Reload()
            except Exception as e:  # malformed stray file — skip this dir, keep the sweep alive
                print(f"[tb2aim] WARN {name}/{rel}: {e}", file=sys.stderr)
                continue
            for tag in acc.Tags().get("scalars", []):
                key = tag if rel == "." else f"{rel}::{tag}"
                last = st["tags"].get(key, -1)
                events = [e for e in acc.Scalars(tag) if e.step > last]
                if not events:
                    continue
                if run is None:
                    run = Run(run_hash=st["hash"], repo=repo,
                              system_tracking_interval=None, capture_terminal_logs=False)
                    if st["hash"] is None:  # first sight of this run: name/group/describe it
                        st["hash"] = run.hash
                        run.name = name
                        run.experiment = experiment_for(name)
                        if name in DESCRIPTIONS:
                            run.description = DESCRIPTIONS[name]
                        run["tb_source"] = os.path.realpath(path)
                for e in events:
                    run.track(float(e.value), name=tag, step=int(e.step))
                st["tags"][key] = events[-1].step
                imported += len(events)
    finally:
        if run is not None:
            run.close()
    return imported


def sweep(tb_root: str, repo: str, state_path: str) -> int:
    state = {}
    if os.path.exists(state_path):
        with open(state_path) as f:
            state = json.load(f)
    total = 0
    for name in sorted(os.listdir(tb_root)):
        if os.path.isdir(os.path.join(tb_root, name)):
            n = sync_run(tb_root, name, repo, state)
            if n:
                print(f"[tb2aim] {name}: +{n} points")
                total += n
    with open(state_path, "w") as f:
        json.dump(state, f, indent=1)
    return total


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--tb-dir", default="/tmp/mtgenv_tb")
    ap.add_argument("--repo", default=os.path.join(os.path.dirname(__file__), "..", "data", "aim"))
    ap.add_argument("--watch", type=int, default=0, metavar="SECONDS",
                    help="re-sync forever every N seconds (0 = one-shot)")
    args = ap.parse_args()

    repo = os.path.abspath(args.repo)
    os.makedirs(repo, exist_ok=True)
    state_path = os.path.join(repo, "tb2aim_state.json")

    while True:
        t0 = time.time()
        total = sweep(args.tb_dir, repo, state_path)
        print(f"[tb2aim] sweep done: {total} new points in {time.time() - t0:.1f}s")
        if not args.watch:
            break
        time.sleep(args.watch)


if __name__ == "__main__":
    main()
