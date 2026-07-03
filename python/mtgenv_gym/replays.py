"""Shared replay recorder — the canonical home for "record one self-play game to data/replays/".

A recorded game runs the current policy on both seats (true self-play) with the engine's replay sink
on, and writes it to ``data/replays/`` tagged ``PPO@<run>:<step>`` so the web lobby's "AI Training
Replays" section groups a run's games into a watchable learning progression.

Both entrypoints use this: ``export_replays.py`` (a standalone run that trains + records) and
``selfplay_train.py`` (records periodically during a normal run via ``--replay-every``). One game per
checkpoint is cheap; we never record the *training* rollout games themselves (throughput).
"""

from __future__ import annotations

import os
import re
import time

from stable_baselines3.common.callbacks import BaseCallback

from mtgenv_gym import MtgEnv
from mtgenv_gym.league import ModelOpponent

# The gitignored on-disk store: ``<repo>/data/replays`` (this file is ``<repo>/python/mtgenv_gym/``).
REPLAY_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "data", "replays"))


def now_ms() -> int:
    return int(time.time() * 1000)


def run_name_of(model) -> str:
    """The TensorBoard run folder minus SB3's ``_N`` suffix, so the replay tag matches the run."""
    logdir = getattr(getattr(model, "logger", None), "dir", None)
    name = os.path.basename(logdir) if logdir else "run"
    return re.sub(r"_\d+$", "", name)


def record_game(model, deck, step, out_dir=REPLAY_DIR, run_name=None, seed=12_345,
                self_play=True, deterministic=False):
    """Record one game with the current policy on seat 0. With ``self_play`` the opponent (seat 1) is
    the *same* policy (agent vs itself); else a random opponent. ``deterministic`` records the greedy
    (argmax) policy — the same play the greedy diagnostics analyze — not the sampled rollout policy.
    Returns the written path (or None if the replay couldn't be serialized — never fatal)."""
    opponent = ModelOpponent(model, deterministic=deterministic) if self_play else "random"
    # Explicit per-game decision cap (defense-in-depth): truncates a between-games non-terminating
    # game to a draw. (Can't catch an in-engine, in-step loop — control never returns to Python.)
    env = MtgEnv(deck=deck, record_replay=True, replay_step=step, opponent=opponent, max_decisions=3000)
    obs, info = env.reset(seed=seed + step)
    done = False
    while not done:
        action, _ = model.predict(obs, action_masks=info["action_mask"], deterministic=deterministic)
        obs, _r, term, trunc, info = env.step(int(action))
        done = term or trunc
    sides = deck.split("_vs_") if "_vs_" in deck else [deck, deck]
    tag = f"PPO@{run_name}:{step}" if run_name else f"PPO@{step}"
    opp = tag if self_play else "random"
    try:
        return env.export_replay(out_dir, now_ms(), names=[tag, opp], decks=sides[:2], run_name=run_name)
    except Exception as e:
        # A replay we can't serialize (e.g. the Selesnya counter-map → "key must be a string", flagged
        # to the engine team) must not kill training — skip it, warn once.
        if not getattr(record_game, "_warned", False):
            print(f"  [replay] export skipped ({type(e).__name__}: {e}) — training continues")
            record_game._warned = True
        return None


class ReplayCheckpoint(BaseCallback):
    """Record one greedy self-play game every ``replay_every`` env-steps (plus an initial step-0 game
    of the random-init policy), during a single continuous ``learn()`` — so the lobby shows a run's
    games as a learning progression. ``deterministic=True`` (default) records greedy play."""

    def __init__(self, deck, replay_every, n_envs, out_dir=REPLAY_DIR, run_name=None,
                 deterministic=True, verbose=0):
        super().__init__(verbose)
        self.deck = deck
        self.out_dir = out_dir
        self.every_calls = max(replay_every // n_envs, 1)
        self.run_name = run_name
        self.deterministic = deterministic

    def _on_training_start(self) -> None:
        if self.run_name is None:
            self.run_name = run_name_of(self.model)
        record_game(self.model, self.deck, 0, self.out_dir, run_name=self.run_name,
                    deterministic=self.deterministic)

    def _on_step(self) -> bool:
        if self.n_calls % self.every_calls == 0:
            path = record_game(self.model, self.deck, self.num_timesteps, self.out_dir,
                               run_name=self.run_name, deterministic=self.deterministic)
            if self.verbose and path:
                print(f"  [replay] step {self.num_timesteps:>7}: {os.path.basename(path)}")
        return True
