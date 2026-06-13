"""Low-level random self-play driver — talks straight to ``mtg_py.PyGame`` (no Gym dependency).

This is the milestone-0 throughput/conservation harness: it plays whole games by picking a
uniformly-random *legal* action at every decision and returns per-game stats + the engine's
conservation summary. Used by both the smoke test and the benchmark script.
"""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np

import mtg_py


@dataclass
class GameStats:
    decisions: int
    min_legal: int          # smallest legal-option count seen (must be >= 1 every decision)
    winner: object          # seat index or None
    turns: int
    reason: str
    # Conservation invariants (from the Rust game thread, which owns the final GameState):
    initial_object_count: int
    object_count: int
    zone_sum: int

    @property
    def cards_conserved(self) -> bool:
        return self.object_count == self.initial_object_count

    @property
    def zones_conserved(self) -> bool:
        return self.zone_sum == self.object_count


def play_random_game(deck="demo", seed=0, auto_pass=True, rng=None) -> GameStats:
    """Play one game to termination with a uniformly-random legal policy on both seats."""
    if rng is None:
        rng = np.random.default_rng(seed)
    game = mtg_py.PyGame(deck, auto_pass)
    obs, mask, seat, request, num_legal, terminal = game.reset(int(seed) & ((1 << 64) - 1))

    decisions = 0
    min_legal = num_legal if not terminal else 1 << 30
    while not terminal:
        legal = np.flatnonzero(np.asarray(mask, dtype=bool))
        assert legal.size >= 1, f"empty action mask at decision {decisions} ({request})"
        min_legal = min(min_legal, num_legal)
        game.apply(int(rng.choice(legal)))
        decisions += 1
        obs, mask, seat, request, num_legal, terminal = game.step_to_decision()

    winner, turns, reason, init_objs, objs, zone_sum = game.summary()
    return GameStats(
        decisions=decisions,
        min_legal=(min_legal if min_legal != (1 << 30) else 0),
        winner=winner,
        turns=turns,
        reason=reason,
        initial_object_count=init_objs,
        object_count=objs,
        zone_sum=zone_sum,
    )
