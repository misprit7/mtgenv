"""Throughput analysis for M2 (GYM_PLAN §8.2). Measures, on the demo deck:

  1. raw engine games/s/core  — random self-play, no NN (the simulator ceiling)
  2. self-play training games/s — DummyVecEnv vs SubprocVecEnv (NN in the loop)

Conclusion (see WORKLOG / the M2c report): the simulator is NOT the bottleneck — NN inference is,
dominated by the *per-env synchronous opponent* `predict`. `SubprocVecEnv` does not help (its
per-step IPC of the large Dict obs over pipes costs more than the parallelism it buys, because the
sim is fast). Hitting the ≥10² games/s/core bar WITH the NN needs **async batched inference** (one
forward over all envs' pending decisions), which is the natural pairing with M3's resumable step
API. Until then, in-process `DummyVecEnv` (batched-learner) is the fastest option.

    PYTHONPATH=python python python/throughput.py
"""

from __future__ import annotations

import glob
import os
import time

import numpy as np

DECK = "demo"
DPG = 75  # measured agent-decisions per demo game


def raw_engine_games_per_sec(seconds=4.0):
    from mtgenv_gym import play_random_game

    t0 = time.time()
    g = 0
    while time.time() - t0 < seconds:
        play_random_game(deck=DECK, seed=g)
        g += 1
    return g / (time.time() - t0)


def selfplay_steps_per_sec(subproc, steps=12000, n_envs=8, warmup=4096):
    from sb3_contrib import MaskablePPO

    from mtgenv_gym.policy import EntityExtractor
    from selfplay_train import make_vecenv

    pool = f"/tmp/mtgenv_pool_tp_{int(subproc)}"
    os.makedirs(pool, exist_ok=True)
    for f in glob.glob(pool + "/*.zip"):
        os.remove(f)
    venv = make_vecenv(DECK, pool, n_envs, 0, subproc=subproc)
    m = MaskablePPO(
        "MultiInputPolicy", venv,
        policy_kwargs=dict(features_extractor_class=EntityExtractor),
        n_steps=256, batch_size=256, verbose=0,
    )
    m.save(pool + "/ckpt_000000000")  # seed the pool so the opponent NN is exercised
    m.learn(total_timesteps=warmup, progress_bar=False, reset_num_timesteps=False)  # absorb startup
    t0 = time.time()
    m.learn(total_timesteps=steps, progress_bar=False, reset_num_timesteps=False)
    dt = time.time() - t0
    venv.close()
    return steps / dt, n_envs


def main():
    raw = raw_engine_games_per_sec()
    print(f"raw engine (no NN):        {raw:6.0f} games/s/core   [{os.cpu_count()} cores]")
    for subproc in (False, True):
        sps, n = selfplay_steps_per_sec(subproc)
        gps = sps / DPG
        tag = f"SubprocVecEnv({n})" if subproc else f"DummyVecEnv({n})"
        # per-core: Dummy ≈ 1 process; Subproc ≈ n workers
        per_core = gps / (n if subproc else 1)
        print(f"self-play {tag:18s}: {sps:6.0f} steps/s  {gps:5.1f} games/s  ({per_core:4.1f} games/s/core)")
    print("\nbottleneck = NN inference (per-env opponent predict), not the sim; "
          "≥10² games/s/core needs async batched inference (M3-adjacent).")


if __name__ == "__main__":
    main()
