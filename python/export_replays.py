"""Export a few training replays across a run so you can *watch the agent learn* (REPLAY_PLAN §3
preview — the full self-play export rides with M2). Records one policy-vs-random game at each
checkpoint to ``data/replays/``, tagged ``AiTraining{step}``, viewable in the web lobby's "AI
Training Replays" section (god-view, step/auto-play).

    PYTHONPATH=python python python/export_replays.py --deck burn_vs_bears
"""

from __future__ import annotations

import argparse
import os
import time

from train import make_model
from mtgenv_gym import MtgEnv

REPLAY_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "data", "replays"))


def _now_ms() -> int:
    return int(time.time() * 1000)


def record_game(model, deck, step, out_dir, seed=12_345):
    """Play one recorded game with the current policy (seat 0) vs random (seat 1); write its replay."""
    env = MtgEnv(deck=deck, record_replay=True, replay_step=step)
    obs, info = env.reset(seed=seed + step)
    done = False
    while not done:
        action, _ = model.predict(obs, action_masks=info["action_mask"], deterministic=False)
        obs, _r, term, trunc, info = env.step(int(action))
        done = term or trunc
    sides = deck.split("_vs_") if "_vs_" in deck else [deck, deck]
    return env.export_replay(
        out_dir, _now_ms(), names=[f"PPO@{step}", "random"], decks=sides[:2] or [deck, deck]
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="burn_vs_bears", choices=["lands", "demo", "burn_vs_bears"])
    ap.add_argument("--checkpoints", default="0,5000,10000,20000,40000,60000",
                    help="comma-separated training-step checkpoints to record at")
    args = ap.parse_args()

    steps = [int(x) for x in args.checkpoints.split(",")]
    model = make_model(deck=args.deck, n_envs=8, seed=0)

    print(f"deck={args.deck}  checkpoints={steps}  → {REPLAY_DIR}")
    done = 0
    t0 = time.time()
    for target in steps:
        if target > done:
            model.learn(total_timesteps=target - done, reset_num_timesteps=False, progress_bar=False)
            done = target
        path = record_game(model, args.deck, done, REPLAY_DIR)
        print(f"  step {done:>6}: {os.path.basename(path)}")
    print(f"\nwrote {len(steps)} replays in {time.time() - t0:.0f}s")
    print("view: run the web server (mtg-gre-server) and open the lobby's 'AI Training Replays' section")


if __name__ == "__main__":
    main()
