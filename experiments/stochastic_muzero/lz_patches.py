"""Local runtime patches for LightZero v0.2.0 bugs that affect Stochastic MuZero.

Import this module before building/training the policy (the config imports it). It patches the
installed package *in memory* only — site-packages stays pristine and reproducible.

Bug 1 — `timestep` kwarg drift (v0.2.0):
    `muzero_collector.py` / `muzero_evaluator.py` call `policy.forward(..., timestep=timestep)`.
    `MuZeroPolicy._forward_collect/_forward_eval` absorb it via `**kwargs`, but the
    `StochasticMuZeroPolicy` equivalents were never given `**kwargs`, so collection/eval crash with
    `TypeError: _forward_eval() got an unexpected keyword argument 'timestep'`. `timestep` is not
    used inside `forward` (only in `_process_transition`, which the workers call separately and which
    StochasticMuZeroPolicy *does* accept), so we simply drop it before delegating to the original.
"""

from __future__ import annotations

import functools


def _apply():
    from lzero.policy.stochastic_muzero import StochasticMuZeroPolicy

    def _strip_timestep(fn):
        if getattr(fn, "_timestep_stripped", False):
            return fn

        @functools.wraps(fn)
        def wrapper(self, *args, **kwargs):
            kwargs.pop("timestep", None)
            return fn(self, *args, **kwargs)

        wrapper._timestep_stripped = True
        return wrapper

    StochasticMuZeroPolicy._forward_collect = _strip_timestep(StochasticMuZeroPolicy._forward_collect)
    StochasticMuZeroPolicy._forward_eval = _strip_timestep(StochasticMuZeroPolicy._forward_eval)


_apply()
