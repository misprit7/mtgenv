"""Export a few training replays across a run so you can *watch the agent learn* (REPLAY_PLAN §3
preview — the full self-play export rides with M2). Trains in one continuous run (clean TensorBoard
curves) and records one policy-vs-random game at checkpoints to ``data/replays/``, tagged
``AiTraining{step}``, viewable in the web lobby's "AI Training Replays" section.

    PYTHONPATH=python python python/export_replays.py --deck burn_vs_bears --tensorboard /tmp/mtgenv_tb
    tensorboard --logdir /tmp/mtgenv_tb     # the run appears as MaskablePPO_<n>
"""

from __future__ import annotations

import argparse
import os
import time

from stable_baselines3.common.callbacks import BaseCallback

from train import eval_callback, make_model
from mtgenv_gym import MtgEnv

REPLAY_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "data", "replays"))


def _now_ms() -> int:
    return int(time.time() * 1000)


def record_game(model, deck, step, out_dir, run_name=None, seed=12_345):
    """Play one recorded game with the current policy (seat 0) vs random (seat 1); write its replay."""
    env = MtgEnv(deck=deck, record_replay=True, replay_step=step)
    obs, info = env.reset(seed=seed + step)
    done = False
    while not done:
        action, _ = model.predict(obs, action_masks=info["action_mask"], deterministic=False)
        obs, _r, term, trunc, info = env.step(int(action))
        done = term or trunc
    sides = deck.split("_vs_") if "_vs_" in deck else [deck, deck]
    label = f"PPO@{run_name}:{step}" if run_name else f"PPO@{step}"
    return env.export_replay(
        out_dir, _now_ms(), names=[label, "random"], decks=sides[:2], run_name=run_name
    )


def _run_name(model) -> str:
    """The TensorBoard run folder (e.g. ``MaskablePPO_2``) so replay filenames match TB runs."""
    logdir = getattr(getattr(model, "logger", None), "dir", None)
    return os.path.basename(logdir) if logdir else "run"


class ReplayCheckpoint(BaseCallback):
    """Record one replay every ``record_every`` env-steps during a single continuous ``learn()``."""

    def __init__(self, deck, out_dir, record_every, n_envs):
        super().__init__()
        self.deck = deck
        self.out_dir = out_dir
        self.every_calls = max(record_every // n_envs, 1)
        self.run_name = "run"

    def _on_training_start(self) -> None:
        self.run_name = _run_name(self.model)
        # An initial, pre-training (random-policy) checkpoint at step 0.
        record_game(self.model, self.deck, 0, self.out_dir, run_name=self.run_name)

    def _on_step(self) -> bool:
        if self.n_calls % self.every_calls == 0:
            path = record_game(
                self.model, self.deck, self.num_timesteps, self.out_dir, run_name=self.run_name
            )
            if self.verbose:
                print(f"  step {self.num_timesteps:>6}: {os.path.basename(path)}")
        return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="demo", choices=["lands", "demo", "burn_vs_bears"])
    ap.add_argument("--timesteps", type=int, default=60_000)
    ap.add_argument("--record-every", type=int, default=10_000, help="record a replay every N steps")
    ap.add_argument("--tensorboard", default="/tmp/mtgenv_tb", help="TensorBoard log dir")
    ap.add_argument("--n-envs", type=int, default=8)
    args = ap.parse_args()

    model = make_model(
        deck=args.deck, n_envs=args.n_envs, seed=0, tensorboard_log=args.tensorboard, verbose=1
    )
    cbs = [
        ReplayCheckpoint(args.deck, REPLAY_DIR, args.record_every, args.n_envs),
        eval_callback(deck=args.deck, eval_freq=max(2000 // args.n_envs, 1)),
    ]
    for c in cbs:
        c.verbose = 1

    print(f"deck={args.deck}  timesteps={args.timesteps}  → replays:{REPLAY_DIR}  tb:{args.tensorboard}")
    t0 = time.time()
    model.learn(total_timesteps=args.timesteps, callback=cbs, progress_bar=False)
    print(f"\ndone in {time.time() - t0:.0f}s — TensorBoard run under {args.tensorboard} (MaskablePPO_*)")
    print("replays: lobby 'AI Training Replays' section (mtg-serve on :8080)")


if __name__ == "__main__":
    main()
