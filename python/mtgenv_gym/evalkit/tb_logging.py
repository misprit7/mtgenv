"""Canonical evalkit logging: ONE TensorBoard tag schema + machine-readable JSON artifacts.

Every algorithm's eval logs the identical tags, so PPO / MuZero / DMC runs overlay in one TensorBoard.
The schema is **backward-compatible** with the live 2.x PPO runs (verified against 2.5/2.9):

    selfplay/winrate_vs_random          selfplay/winrate_vs_random_sampled   (sampled variant)
    selfplay/winrate_vs_initial         selfplay/winrate_vs_initial_sampled
    selfplay/winrate_vs_script          selfplay/winrate_vs_script_sampled   (vs the scripted yardstick)
    ladder/winrate_vs_10pct  … _75pct   (+ _100pct if the milestone is used)
    stats/<attack_rate|productive_rate|block_rate|cast_rate|playland_rate|block_double_rate>
    game/turns_mean   game/end_<reason>
    <deck>/<analyzer-scalar>            (e.g. swine/chump_rate_swine_hi)

**Rotating eval seeds (contract from 2026-07-08).** Every eval's game seed ROTATES with the training
step (``eval_seed(base, step) = base + step``): consecutive evals sample DIFFERENT games instead of
replaying one frozen test set, so a converged fixed-seed policy no longer draws an exactly flat line
that invites wrong conclusions. Distinct per-opponent ``base``s (random 5e6 / initial 6e6 / ladder
7e6+pct / script 8e6) keep opponents on independent shuffles; the step jumps by ``eval_freq`` (≥
``n_games``) each eval so the ``[seed, seed+n_games)`` ranges never overlap. Rotating the ``Arena`` seed
moves BOTH the engine shuffle AND the opponent policy's rng in one place (``Arena`` reseeds the opponent
per game from ``seed+g``). Each eval's resolved ``seed`` is recorded in its JSON, so it stays exactly
reproducible. (Levels unchanged — both estimate the true win-rate; only the noise is unfrozen. Runs
before this contract, e.g. 4.4-4.6 and the 3.x/model-based runs, are fixed-seed.)

Two backends behind one ``Recorder`` interface so the same call site serves both a training callback
and an offline backfill: :class:`SB3Recorder` (wraps SB3's ``logger.record``; SB3 owns the step) and
:class:`WriterRecorder` (wraps a torch ``SummaryWriter``; step is explicit — for the CLI / any custom
loop). Beside TB, every eval is also written as JSON under ``<run_dir>/evalkit/eval_stepNNN.json`` so
there is a machine-readable eval history independent of the TB event files.
"""

from __future__ import annotations

import json
import os
from typing import Protocol

from .arena import EvalResult


def eval_seed(base: int, step: "int | None") -> int:
    """Rotating per-eval game seed: ``base + step`` (see module docstring). ``step`` None → ``base``."""
    return int(base) + int(step or 0)


class Recorder(Protocol):
    def record(self, tag: str, value: float, step: "int | None" = None) -> None: ...


class SB3Recorder:
    """Record into an SB3 ``Logger`` (``self.logger`` in a callback). SB3 dumps at rollout end keyed by
    its own ``num_timesteps`` — ``step`` is ignored here (SB3 controls it)."""

    def __init__(self, logger):
        self._logger = logger

    def record(self, tag, value, step=None):
        self._logger.record(tag, float(value))


class WriterRecorder:
    """Record into a torch ``SummaryWriter`` at an explicit ``step`` (offline CLI / custom loops)."""

    def __init__(self, writer):
        self._w = writer

    def record(self, tag, value, step=None):
        self._w.add_scalar(tag, float(value), int(step) if step is not None else 0)

    def flush(self):
        self._w.flush()


# ── the schema ────────────────────────────────────────────────────────────────────────────────────
def log_eval(recorder: Recorder, results: "dict[str, EvalResult]", *, win_tag: str,
             step: "int | None" = None, with_stats: bool = True, with_game: bool = True,
             with_analyzers: bool = True) -> "dict[str, float]":
    """Log a ``{mode: EvalResult}`` bundle under the canonical schema and return the flat tag→value
    dict written (handy for tests / smoke tag-set comparison).

    * ``results["greedy"]`` → ``win_tag`` (+ ``stats/*`` + ``game/*`` + analyzer scalars).
    * ``results["sample"]`` → ``win_tag + "_sampled"`` (win-rate only — the honest learning curve).

    ``win_tag`` is the opponent-specific base, e.g. ``"selfplay/winrate_vs_random"`` or
    ``"ladder/winrate_vs_25pct"``. Ladder milestones pass ``with_stats=with_game=False`` (win-rate
    only, matching the legacy ``ladder/*``)."""
    written: "dict[str, float]" = {}

    def rec(tag, value):
        recorder.record(tag, value, step)
        written[tag] = float(value)

    greedy = results.get("greedy")
    if greedy is not None:
        rec(win_tag, greedy.win_rate)
        if with_stats:
            for k, v in greedy.stats.items():
                rec(f"stats/{k}", v)
        if with_game:
            rec("game/turns_mean", greedy.avg_turns)
            for r, frac in greedy.end_reasons.items():
                rec(f"game/end_{r}", frac)
        if with_analyzers:
            for k, v in greedy.analyzers.items():
                rec(k, v)  # analyzer keys are already namespaced (e.g. "swine/…")

    sample = results.get("sample")
    if sample is not None:
        rec(f"{win_tag}_sampled", sample.win_rate)
    return written


# ── JSON artifacts ─────────────────────────────────────────────────────────────────────────────────
def write_json(run_dir: str, step: int, labelled: "dict[str, EvalResult]",
               extra: "dict | None" = None) -> str:
    """Write ``<run_dir>/evalkit/eval_step<step>.json`` = ``{step, results:{label: EvalResult…}, …}``.
    ``labelled`` keys are free-form (e.g. ``"random_greedy"``, ``"initial_sampled"``)."""
    d = os.path.join(run_dir, "evalkit")
    os.makedirs(d, exist_ok=True)
    payload = {"step": int(step), "results": {k: v.to_json() for k, v in labelled.items()}}
    if extra:
        payload.update(extra)
    path = os.path.join(d, f"eval_step{int(step):09d}.json")
    with open(path, "w") as f:
        json.dump(payload, f, indent=2)
    return path
