"""Policy-driven replay recording — record one game to ``data/replays/`` via the ``Policy`` protocol.

Generalizes the two existing recorders (``mtgenv_gym.replays.record_game`` for SB3 and the MuZero
harness's ``record_replay``) into one algorithm-agnostic function: drive a greedy game with any
``Policy`` on both seats and export the omniscient replay JSON with the standard ``aitrain`` naming, so
every algorithm's games appear in the web lobby's "AI Training Replays" section side by side.

Import-light (no sb3/torch at module load — mirrors ``muzero_observability``'s local ``REPLAY_DIR``) so
it works in any venv; ``MtgEnv``/``mtg_py`` is the only dependency.
"""

from __future__ import annotations

import os
import time

from ..env import MtgEnv
from .policy import RandomPolicy

_U64 = (1 << 64) - 1

# <repo>/data/replays — this file is <repo>/python/mtgenv_gym/evalkit/replay.py (matches
# mtgenv_gym.replays.REPLAY_DIR without importing it, since that module pulls sb3).
REPLAY_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "data", "replays"))


def now_ms() -> int:
    return int(time.time() * 1000)


def record_game(policy, deck: str, step: int, *, opponent=None, self_play: bool = False,
                out_dir: str = REPLAY_DIR, run_name: "str | None" = None, algo: str = "PPO",
                seed: int = 12_345, max_decisions: int = 3000) -> "str | None":
    """Record one greedy game with ``policy`` on seat 0 and write it to ``out_dir``.

    Opponent (seat 1): ``policy`` itself if ``self_play`` (names = ``[tag, tag]``); a
    :class:`RandomPolicy` if ``opponent`` is None (names = ``[tag, "random"]`` — bounded, matches the
    win-rate-vs-random protocol and avoids the two-passers huge-replay problem for a weak policy); or an
    explicit ``opponent`` Policy. ``algo`` tags the run (``PPO@…`` / ``MuZero@…``). Returns the written
    path, or ``None`` if nothing could be serialized (never fatal — mirrors both source recorders)."""
    tag = f"{algo}@{run_name}:{step}" if run_name else f"{algo}@{step}"
    if self_play:
        opp, opp_label = policy, tag
    elif opponent is None:
        opp, opp_label = RandomPolicy(seed=seed), "random"
    else:
        opp, opp_label = opponent, "opponent"

    env = MtgEnv(deck=deck, record_replay=True, replay_step=int(step), opponent="external",
                 max_decisions=max_decisions)
    env.ext_reset((seed + int(step)) & _U64)
    for pol in (policy, opp):  # per-slot RNG for a RandomPolicy seat
        reset = getattr(pol, "reset", None)
        if reset is not None:
            try:
                reset([0], game_seeds=[seed + int(step)])
            except TypeError:
                reset([0])
    for _ in range(200_000):
        st = env.ext_state()
        if st == "terminal":
            break
        actor = policy if st == "learner" else opp
        a = actor.act([env.ext_obs()], [env.ext_mask()], mode="greedy", env_indices=[0])[0]
        env.ext_apply(int(a))

    sides = deck.split("_vs_") if "_vs_" in deck else [deck, deck]
    try:
        return env.export_replay(out_dir, now_ms(), names=[tag, opp_label], decks=sides[:2],
                                 run_name=run_name)
    except Exception as e:
        if not getattr(record_game, "_warned", False):
            print(f"  [evalkit.replay] export skipped ({type(e).__name__}: {e}) — continuing")
            record_game._warned = True
        return None
