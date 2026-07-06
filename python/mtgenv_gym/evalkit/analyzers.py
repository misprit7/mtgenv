"""Pluggable per-deck judgment analyzers.

Some decks have a *specific* strategic question that a generic win-rate/attack-rate can't see. The
swine deck's is the user's chump-block concern: does the policy trade a 2/2 into a trampling 3/3 even
at high life (where you should just take 3)? An analyzer watches the evaluated policy's decisions
during Arena play and emits deck-specific scalars, logged automatically **when the deck matches** — no
caller wiring. Adding one is a factory in ``ANALYZERS`` (or ``register_analyzer``).

An analyzer sees, per finalized decision of the evaluated policy, the ``obs`` it acted on (pre-apply)
and the ``decision_stats`` record that decision produced — the same two things ``swine_blocks.py``
reads. Output is a flat ``{tag: value}`` dict; tags are logged verbatim under the run and carried in
``EvalResult.analyzers``.
"""

from __future__ import annotations

from typing import Callable, Protocol, runtime_checkable

import numpy as np


@runtime_checkable
class Analyzer(Protocol):
    name: str

    def observe(self, obs: "dict[str, np.ndarray]", record: dict) -> None: ...
    def result(self) -> "dict[str, float]": ...
    def reset(self) -> None: ...


# ── swine: chump-blocking a trampler at high life ────────────────────────────────────────────────
_MY_LIFE = 16                      # globals index (matches swine_blocks._MY_LIFE / _G_MY_LIFE)
_BF_PRESENT, _BF_MINE, _BF_POWER = 0, 1, 2


def _swine_attacking(bf: np.ndarray) -> bool:
    """True if an ENEMY (is_mine=0) power-3 creature is attacking — a trampling Swine (bears are 2/2).
    ``attacking`` is the width-5 column of ``bf_feat`` (matches ``swine_blocks._swine_attacking``)."""
    atk = bf.shape[1] - 5
    m = ((bf[:, _BF_PRESENT] > 0.5) & (bf[:, _BF_MINE] < 0.5)
         & (bf[:, _BF_POWER] == 3) & (bf[:, atk] > 0.5))
    return bool(m.any())


class SwineBlockAnalyzer:
    """Chump/gang-at-high-life signal (the swine experiment, mirrors ``swine_blocks.py``).

    At each DeclareBlockers decision (``block_eligible > 0``) records the blocker's life, whether it
    blocked, whether it ganged (≥2 blockers on one attacker — the sophisticated anti-trample play),
    and whether a Swine was attacking. The concerning signal: high ``block_rate`` in "Swine attacking
    & life ≥ life_hi" with low ``gang_rate`` = single-blocking a trampler it should just take."""

    name = "swine"

    def __init__(self, life_hi: int = 15):
        self.life_hi = int(life_hi)
        self.reset()

    def reset(self) -> None:
        # rows: (life, blocked, gang, attackers_blocked, swine_attacking)
        self._rows: "list[tuple[float, float, float, float, float]]" = []

    def observe(self, obs, record) -> None:
        if not record or record.get("block_eligible", 0) <= 0:
            return
        life = float(np.asarray(obs["globals"]).ravel()[_MY_LIFE])
        swine = _swine_attacking(np.asarray(obs["bf_feat"]))
        self._rows.append((
            life,
            1.0 if record.get("block_declared", 0) > 0 else 0.0,
            1.0 if record.get("block_double", 0) > 0 else 0.0,
            float(record.get("attackers_blocked", 0)),
            1.0 if swine else 0.0,
        ))

    def result(self) -> "dict[str, float]":
        r = np.array(self._rows) if self._rows else np.empty((0, 5))
        out = {"swine/n_block_decisions": float(len(r))}
        if len(r) == 0:
            return out
        life, blocked, gang, _abl, swine = r[:, 0], r[:, 1], r[:, 2], r[:, 3], r[:, 4] > 0.5
        hi = life >= self.life_hi

        def _blk(mask):
            return float(blocked[mask].mean()) if mask.any() else float("nan")

        def _gang(mask):  # of decisions where it blocked, fraction that ganged
            bm = mask & (blocked > 0.5)
            return float(gang[bm].mean()) if bm.any() else float("nan")

        out.update({
            "swine/block_rate_hi_life": _blk(hi),
            "swine/block_rate_lo_life": _blk(~hi),
            # the user's exact concern: blocking a trampler at high life, and not even ganging it.
            "swine/chump_rate_swine_hi": _blk(swine & hi),
            "swine/gang_rate_swine_hi": _gang(swine & hi),
            "swine/block_rate_no_swine": _blk(~swine),
        })
        return out


# ── registry ─────────────────────────────────────────────────────────────────────────────────────
ANALYZERS: "dict[str, Callable[[], Analyzer]]" = {
    "swine": SwineBlockAnalyzer,
}


def register_analyzer(deck: str, factory: "Callable[[], Analyzer]") -> None:
    """Register (or override) the analyzer factory for ``deck``."""
    ANALYZERS[deck] = factory


def get_analyzer(deck: str) -> "Analyzer | None":
    """The analyzer for ``deck`` (a fresh instance), or ``None`` if the deck has none."""
    factory = ANALYZERS.get(deck)
    return factory() if factory is not None else None
