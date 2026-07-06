"""Metric aggregation for an eval run — win-rate + Wilson CI, average turns, end-reason mix, and the
per-decision behaviour ratios (attack/productive/block/…).

The behaviour ratios reuse ``mtgenv_gym.tracked_stats`` **exactly** — same ``StatDef`` registry, same
``StatAccumulator`` accumulation — so an eval game and a training rollout report the identical metric
under the identical definition (no drift between "what training logs" and "what eval logs"). Adding a
stat is still one ``StatDef`` in ``tracked_stats.py``; it flows here for free.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field

from ..tracked_stats import REGISTRY, StatAccumulator


def wilson_ci(wins: int, n: int, z: float = 1.96) -> "tuple[float, float]":
    """95%-by-default Wilson score interval for a binomial proportion (``wins`` of ``n``). Robust at
    small ``n`` and at rates near 0/1 where the normal approximation degenerates. ``n == 0 → (0, 0)``."""
    if n <= 0:
        return (0.0, 0.0)
    p = wins / n
    z2 = z * z
    denom = 1.0 + z2 / n
    center = (p + z2 / (2 * n)) / denom
    half = (z * math.sqrt((p * (1 - p) + z2 / (4 * n)) / n)) / denom
    return (max(0.0, center - half), min(1.0, center + half))


@dataclass
class OutcomeAgg:
    """Accumulates game outcomes (from the evaluated policy's perspective) + length/reason mix."""

    wins: int = 0
    losses: int = 0
    draws: int = 0
    turns: "list[int]" = field(default_factory=list)
    reasons: "dict[str, int]" = field(default_factory=dict)

    def add(self, reward: float, summary: "dict | None") -> None:
        if reward > 0.5:
            self.wins += 1
        elif reward < -0.5:
            self.losses += 1
        else:
            self.draws += 1
        if summary:
            self.turns.append(int(summary.get("turns", 0)))
            r = str(summary.get("reason", "?"))
            self.reasons[r] = self.reasons.get(r, 0) + 1

    @property
    def n(self) -> int:
        return self.wins + self.losses + self.draws

    def win_rate(self) -> float:
        return self.wins / self.n if self.n else float("nan")

    def avg_turns(self) -> float:
        return (sum(self.turns) / len(self.turns)) if self.turns else float("nan")

    def end_reason_fracs(self) -> "dict[str, float]":
        total = sum(self.reasons.values()) or 1
        return {r: c / total for r, c in self.reasons.items()}


class StatsAgg:
    """Thin wrapper over ``tracked_stats.StatAccumulator`` yielding a plain ``{name: ratio}`` dict
    (no ``stats/`` prefix, no raw num/den) for the evaluated policy's decisions."""

    def __init__(self, registry=REGISTRY):
        self._acc = StatAccumulator(registry)

    def update(self, record: dict) -> None:
        self._acc.update(record)

    def as_dict(self) -> "dict[str, float]":
        out = {}
        for name, v in self._acc.as_log_dict().items():
            if name.endswith("_num") or name.endswith("_den"):
                continue
            out[name[len("stats/"):]] = v  # "stats/attack_rate" -> "attack_rate"
        return out
