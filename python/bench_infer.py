"""Microbench: does a batched policy forward cost ~the same as a single one?

This validates the premise of async batched inference — if forward latency is ~flat from batch 1→N,
then collecting N envs' opponent decisions into one forward is ~Nx cheaper than N separate calls.

    PYTHONPATH=python python/.venv/bin/python python/bench_infer.py
"""
from __future__ import annotations

import time

import numpy as np
import torch
from sb3_contrib import MaskablePPO
from stable_baselines3.common.vec_env import DummyVecEnv

from mtgenv_gym import MtgEnv
from mtgenv_gym.policy import EntityExtractor


def _stack(obs, n):
    return {k: np.repeat(v[None], n, axis=0) for k, v in obs.items()}


def main():
    venv = DummyVecEnv([lambda: MtgEnv(deck="demo")])
    model = MaskablePPO(
        "MultiInputPolicy", venv,
        policy_kwargs=dict(features_extractor_class=EntityExtractor),
        n_steps=64, batch_size=64, verbose=0,
    )
    policy = model.policy
    dev = policy.device
    print(f"device: {dev}")

    env = MtgEnv(deck="demo")
    obs, info = env.reset(seed=0)
    mask = info["action_mask"]

    iters = 300
    for n in (1, 4, 8, 16, 32, 64, 128):
        bobs = _stack(obs, n)
        bmask = np.repeat(mask[None], n, axis=0)
        # warmup
        for _ in range(10):
            policy.predict(bobs, action_masks=bmask, deterministic=True)
        if dev.type == "cuda":
            torch.cuda.synchronize()
        t0 = time.time()
        for _ in range(iters):
            policy.predict(bobs, action_masks=bmask, deterministic=True)
        if dev.type == "cuda":
            torch.cuda.synchronize()
        dt = (time.time() - t0) / iters
        print(f"batch {n:4d}: {dt*1e3:7.3f} ms/forward   {dt*1e3/n:7.4f} ms/sample   "
              f"({n/dt:8.0f} samples/s)")


if __name__ == "__main__":
    main()
