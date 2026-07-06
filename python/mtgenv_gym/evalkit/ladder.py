"""``Ladder`` — the %-trained self-relative progress curve, framework-managed for ANY algorithm.

Win-rate vs random saturates fast; a policy still improving keeps beating its *earlier selves*. The
ladder freezes the policy at fixed training-fraction milestones (10/25/50/75%, optionally 100%) and
logs the current policy's win-rate vs each frozen snapshot (``ladder/winrate_vs_NNpct``). A milestone
not yet reached logs ``NaN`` (a gap in TB, not an error) until training crosses it.

Algorithm-agnostic via two seams the caller supplies:

* ``snapshot_fn(path) -> saved_path`` — freeze the *current* policy to ``path`` (e.g. ``model.save``
  for SB3, ``torch.save(state_dict)`` for a custom net). Called once per milestone as it's reached.
* ``load_policy_fn(saved_path) -> Policy`` — load a frozen snapshot back as an opponent ``Policy``.

So the ladder works for PPO, MuZero, DMC, … unchanged — only these two closures differ.
"""

from __future__ import annotations

import os
from typing import Callable

_LADDER_SEED_BASE = 7_000_000  # matches legacy LadderEval (seed = 7_000_000 + pct)


class Ladder:
    def __init__(self, save_dir: str, snapshot_fn: "Callable[[str], str | None]",
                 load_policy_fn: "Callable[[str], object]", *,
                 milestones=(0.10, 0.25, 0.50, 0.75), n_games: int = 40,
                 seed_base: int = _LADDER_SEED_BASE):
        self.save_dir = save_dir
        self.snapshot_fn = snapshot_fn
        self.load_policy_fn = load_policy_fn
        self.milestones = tuple(milestones)
        self.n_games = int(n_games)
        self.seed_base = int(seed_base)
        self._snap: "dict[int, str]" = {}    # pct -> saved snapshot path
        self._loaded: "dict[int, object]" = {}  # pct -> loaded opponent Policy (cache)
        os.makedirs(save_dir, exist_ok=True)

    @staticmethod
    def _pct(m: float) -> int:
        return int(round(m * 100))

    def maybe_snapshot(self, frac: float) -> None:
        """If training has reached a not-yet-snapshotted milestone, freeze the current policy there."""
        for m in self.milestones:
            pct = self._pct(m)
            if pct not in self._snap and frac >= m:
                path = os.path.join(self.save_dir, f"ladder_{pct:02d}")
                saved = self.snapshot_fn(path)
                self._snap[pct] = saved or path

    def _opponent(self, pct: int):
        if pct not in self._loaded:
            self._loaded[pct] = self.load_policy_fn(self._snap[pct])
        return self._loaded[pct]

    def eval_and_log(self, policy, arena, recorder, *, step: "int | None" = None,
                     n_games: "int | None" = None) -> "dict[str, float]":
        """For each milestone: greedy win-rate of ``policy`` vs that frozen snapshot (unreached → NaN).
        Logs ``ladder/winrate_vs_NNpct`` (win-rate only, matching legacy) and returns the tag dict."""
        n = n_games or self.n_games
        tags: "dict[str, float]" = {}
        for m in self.milestones:
            pct = self._pct(m)
            tag = f"ladder/winrate_vs_{pct:02d}pct"
            if pct in self._snap:
                res = arena.play(policy, self._opponent(pct), n_games=n,
                                 seed=self.seed_base + pct, a_mode="greedy", b_mode="sample",
                                 opponent_label=f"{pct}pct")
                val = res.win_rate
            else:
                val = float("nan")  # not reached yet → gap in TB
            recorder.record(tag, val, step)
            tags[tag] = val
        return tags
