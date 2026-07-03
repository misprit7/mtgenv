"""M3 acceptance: engine behavioral-equivalence harness (transport-agnostic).

The engine is deterministic by seed, so a fixed ``(deck, seed, scripted_policy)`` tuple must produce
an *identical decision trajectory* no matter which transport drives it — today's ``mtg_py.PyGame``
(thread + channel ``GameConn``) or M3's future ``Session``/fleet path. This harness records a compact
per-decision fingerprint (seat, request kind, num legal actions, chosen action) + final outcome, over
a fixed suite of games, and hashes it. Snapshot the current transport's fingerprints (committed); when
the fleet lands, re-run with its driver and diff — any divergence is behavior drift, caught in seconds.

**Transport seam.** A *driver* is any object with ``decision() -> Decision`` and ``apply(action)``.
``fingerprint_suite(SPECS, make_driver=pygame_driver)`` takes the driver factory as one argument, so
plugging the fleet in later is a one-liner: ``fingerprint_suite(SPECS, make_driver=fleet_driver)``.
The scripted policy is a pure function of (decision-index, mask) — no RNG state to desync — so any
fingerprint difference is the engine's, not the harness's.
"""

from __future__ import annotations

import hashlib
from dataclasses import dataclass

import numpy as np

# Fixed game suite: a few decks × seeds spanning the mechanics (deck-out, race, combat+blocking,
# burn). Small + bounded so the committed snapshot stays compact.
SPECS = [
    ("bears", 0), ("bears", 1),
    ("heralds", 0),
    ("demo", 0),
    ("burn_vs_bears", 0),
    ("lands", 0),
]
MAX_DECISIONS = 2000  # hard cap so the known engine non-termination bug can't hang the harness


@dataclass(frozen=True)
class Decision:
    seat: int
    request: str
    num_legal: int
    mask: np.ndarray
    terminal: bool
    summary: dict | None


class _PyGameDriver:
    """Current transport: ``mtg_py.PyGame`` (thread + channel ``GameConn``) — exactly what ``MtgEnv``
    drives. Constructed per game; ``decision()`` reads the current pause, ``apply()`` advances."""

    name = "pygame-thread-channel"

    def __init__(self, deck, seed):
        import mtg_py

        self.g = mtg_py.PyGame(deck, True, False, 0)  # (deck, auto_pass, record_replay, replay_step)
        self._step = self.g.reset(int(seed))

    def decision(self) -> Decision:
        obs, mask, seat, request, num_legal, terminal = self._step
        summary = None
        if terminal:
            s = self.g.summary()  # (winner, turns, reason, init_objs, objs, zone_sum) | None
            if s is not None:
                summary = {"winner": s[0], "turns": s[1], "reason": s[2]}
        return Decision(int(seat), str(request), int(num_legal),
                        np.asarray(mask, dtype=bool), bool(terminal), summary)

    def apply(self, action: int) -> None:
        self.g.apply(int(action))
        self._step = self.g.step_to_decision()


def pygame_driver(deck, seed):
    """The default (current-path) driver factory — the transport seam's baseline implementation."""
    return _PyGameDriver(deck, seed)


def scripted_action(decision_index: int, mask: np.ndarray) -> int:
    """Deterministic, transport-independent policy: a pure function of (decision index, mask). Picks a
    legal action with a fixed hash so it varies through a game yet never depends on RNG state (which
    could desync across transports and mask a real engine difference)."""
    legal = np.flatnonzero(mask)
    idx = (decision_index * 2654435761 + int(mask.sum()) * 40503) % len(legal)
    return int(legal[idx])


def game_fingerprint(deck, seed, make_driver=pygame_driver) -> dict:
    """Drive one game with the scripted policy on every seat; return its compact fingerprint:
    the per-decision (seat, request, num_legal, action) trace, the final outcome, and a digest."""
    drv = make_driver(deck, seed)
    trace = []
    outcome = None
    truncated = False
    for i in range(MAX_DECISIONS + 1):
        d = drv.decision()
        if d.terminal:
            s = d.summary or {}
            outcome = {"winner": s.get("winner"), "turns": s.get("turns"), "reason": s.get("reason")}
            break
        if i == MAX_DECISIONS:
            truncated = True
            break
        a = scripted_action(i, d.mask)
        trace.append([d.seat, d.request, d.num_legal, a])
        drv.apply(a)
    canon = repr([deck, seed, trace, outcome, truncated]).encode()
    return {
        "deck": deck, "seed": seed, "n_decisions": len(trace),
        "outcome": outcome, "truncated": truncated,
        "digest": hashlib.sha256(canon).hexdigest()[:16],
        "trace": trace,
    }


def fingerprint_suite(specs=SPECS, make_driver=pygame_driver) -> list[dict]:
    """Fingerprint every ``(deck, seed)`` in ``specs`` through ``make_driver`` — the whole acceptance
    payload. Swap ``make_driver`` to score a new transport against the committed snapshot."""
    return [game_fingerprint(deck, seed, make_driver) for (deck, seed) in specs]


def diff_suites(expected: list[dict], actual: list[dict]) -> list[str]:
    """Human-readable divergences between two suites (empty ⇒ byte-identical behavior). Pinpoints the
    first differing decision per game so a transport regression is localized immediately."""
    out = []
    by_key = {(f["deck"], f["seed"]): f for f in actual}
    for exp in expected:
        key = (exp["deck"], exp["seed"])
        act = by_key.get(key)
        if act is None:
            out.append(f"{key}: missing in actual")
            continue
        if act["digest"] == exp["digest"]:
            continue
        # digests differ — find the first divergent decision.
        et, at = exp.get("trace", []), act.get("trace", [])
        first = next((i for i in range(min(len(et), len(at))) if et[i] != at[i]), None)
        if first is not None:
            out.append(f"{key}: decision #{first} {et[first]} → {at[first]}")
        elif len(et) != len(at):
            out.append(f"{key}: length {len(et)} → {len(at)} (outcome {exp['outcome']} → {act['outcome']})")
        else:
            out.append(f"{key}: outcome {exp['outcome']} → {act['outcome']}")
    return out
