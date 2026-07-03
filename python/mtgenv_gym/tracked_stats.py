"""Encapsulated, extensible summary-statistic tracking for TensorBoard (gym task #68).

Each tracked stat is a **ratio** ``numerator / denominator`` accumulated over a rollout from the
env's per-decision semantic records. mtg-py emits one record per *finalized* engine decision
(``PyGame.take_decision_stats`` → ``info["decision_stats"]``); a record is a flat ``field → value``
dict of opportunity/taken counters (see ``crates/mtg-py/src/decision_stats.rs``). A stat is a small
predicate over that record returning ``(num_delta, den_delta)``, so **adding a stat is one
``StatDef`` entry** — no plumbing changes.

Examples (the initial registry):
- ``cast_rate``   = cast_taken / cast_legal      (fraction of priority windows with a legal Cast where one was cast)
- ``attack_rate`` = attack_declared / attack_eligible
- ``block_rate``  = block_declared  / block_eligible

Usage: add ``TrackedStatsCallback()`` to a MaskablePPO ``learn(callback=...)`` list. It reads
``info["decision_stats"]`` each step, accumulates, and logs ``stats/<name>`` (+ raw num/den counts)
to the SB3 logger at every rollout end, then resets for the next rollout.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable, Iterable

from stable_baselines3.common.callbacks import BaseCallback

# A per-decision record is a flat dict {field: float}. A stat maps it to (numerator, denominator)
# deltas; a field absent from this decision contributes 0 (``.get(..., 0.0)``).
Record = dict
StatFn = Callable[[Record], "tuple[float, float]"]


@dataclass(frozen=True)
class StatDef:
    """A named ratio stat: ``fn(record) -> (numerator_delta, denominator_delta)``."""

    name: str
    fn: StatFn


def _ratio(num_key: str, den_key: str) -> StatFn:
    """A stat that is simply ``sum(record[num_key]) / sum(record[den_key])`` over the rollout."""
    return lambda r: (float(r.get(num_key, 0.0)), float(r.get(den_key, 0.0)))


# THE extension point: add a StatDef here and it shows up in TensorBoard. New stats that reuse
# existing record fields need nothing else; a genuinely new measurement also adds a field in
# decision_stats.rs (the only place that reads the raw DecisionRequest/Response).
REGISTRY: list[StatDef] = [
    StatDef("cast_rate", _ratio("cast_taken", "cast_legal")),
    StatDef("attack_rate", _ratio("attack_declared", "attack_eligible")),
    StatDef("block_rate", _ratio("block_declared", "block_eligible")),
    StatDef("playland_rate", _ratio("playland_taken", "playland_legal")),
    # `productive_rate` = took a cast/land/activate when at least one was legal (vs passing). Unlike
    # cast_rate/playland_rate — which are per-window selection rates between mutually-exclusive
    # productive actions and so cap below 1.0 for optimal play — this → 1.0 for a policy that never
    # squanders a priority window. It's OR-combined per window in decision_stats.rs (not derivable
    # from the summed cast/land/activate fields), which is why it needs its own record fields.
    StatDef("productive_rate", _ratio("productive_taken", "productive_legal")),
    # Fraction of blocked attackers that were DOUBLE-blocked (≥2 blockers ganging one attacker). On a
    # trample deck (swine) this is the sophisticated play — gang the 3/3 instead of chumping it — so it
    # separates "single-block everything" (→0) from "gang the trampler" (→>0) even when block_rate=1.0.
    StatDef("block_double_rate", _ratio("block_double", "attackers_blocked")),
]


class StatAccumulator:
    """Running numerator/denominator per stat over a rollout."""

    def __init__(self, registry: Iterable[StatDef] = REGISTRY):
        self.registry = list(registry)
        self.reset()

    def reset(self) -> None:
        self.num = {s.name: 0.0 for s in self.registry}
        self.den = {s.name: 0.0 for s in self.registry}

    def update(self, record: Record) -> None:
        for s in self.registry:
            n, d = s.fn(record)
            self.num[s.name] += n
            self.den[s.name] += d

    def as_log_dict(self) -> dict:
        """``{stats/<name>: ratio, stats/<name>_num: .., stats/<name>_den: ..}``. Ratio is NaN until
        the denominator is observed (a gap in TB, not a divide-by-zero)."""
        out = {}
        for s in self.registry:
            d = self.den[s.name]
            out[f"stats/{s.name}"] = (self.num[s.name] / d) if d > 0 else float("nan")
            out[f"stats/{s.name}_num"] = self.num[s.name]
            out[f"stats/{s.name}_den"] = self.den[s.name]
        return out


class TrackedStatsCallback(BaseCallback):
    """Accumulate per-decision records from ``info["decision_stats"]`` and log ratios each rollout."""

    def __init__(self, registry: Iterable[StatDef] = REGISTRY, verbose: int = 0):
        super().__init__(verbose)
        self.acc = StatAccumulator(registry)

    def _on_step(self) -> bool:
        for info in self.locals.get("infos", []):
            rec = info.get("decision_stats")
            if rec:
                self.acc.update(rec)
        return True

    def _on_rollout_end(self) -> None:
        for k, v in self.acc.as_log_dict().items():
            self.logger.record(k, v)
        self.acc.reset()
