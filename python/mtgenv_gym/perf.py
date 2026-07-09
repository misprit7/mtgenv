"""Boundary-timer performance metrics → a ``perf/`` TB tag family (the dashboard auto-groups by prefix).

HARD CONSTRAINT (user): zero measurable training overhead — **boundary timers only**. No per-decision
or per-env-step instrumentation of the pump internals (that stays in offline profiling, scripts/
profile_train.py). The only hot-ish reads are 2 ``perf_counter`` calls per VEC-step inside the vec env's
``step_wait`` (thousands of reads per run, not millions) for the env-vs-forward split; everything else
fires at iteration / eval / replay boundaries.

Tags (all via ``self.logger`` so SB3 dumps them to TB each iteration):
  perf/rollout_s        collect_rollouts wall time (INCLUDES eval/replay callbacks that fire in on_step)
  perf/train_s          PPO update wall time (rollout_end → next rollout_start)
  perf/env_step_ms      mean vec-env step time this rollout (from the step_wait accumulators)
  perf/fps_rolling      env-steps / iteration wall (consecutive rollout_end deltas)
  perf/elapsed_min      wall clock since training start
  perf/rss_gb           resident memory (psutil)              — at rollout boundaries
  perf/gpu_mem_gb       torch.cuda.memory_allocated (if cuda) — at rollout boundaries
  perf/eval_s / perf/eval_cum_pct / perf/replay_s   logged by EvalkitCallback (it owns those calls)
"""
from __future__ import annotations

import os
import time

from stable_baselines3.common.callbacks import BaseCallback

import torch


def _rss_gb():
    """Resident memory in GB — zero-dependency: psutil if present, else Linux /proc/self/statm (current
    RSS). Returns None if neither works (perf/rss_gb is then simply not logged)."""
    try:
        import psutil
        return psutil.Process().memory_info().rss / 1e9
    except Exception:
        pass
    try:
        with open("/proc/self/statm") as f:
            resident_pages = int(f.read().split()[1])
        return resident_pages * os.sysconf("SC_PAGE_SIZE") / 1e9
    except Exception:
        return None


class PerfCallback(BaseCallback):
    """Iteration-boundary perf timers. Reads the vec env's ``_perf_env_s``/``_perf_env_n`` accumulators
    (set in ``step_wait``) for the env-step split; everything else from the SB3 rollout/train seams."""

    def _base_env(self):
        env = self.training_env
        while hasattr(env, "venv"):
            env = env.venv
        return env

    def _on_training_start(self) -> None:
        self._t0 = time.perf_counter()
        self._roll_start = None
        self._train_start = None
        self._last_end = None
        self._last_ts = 0

    def _on_rollout_start(self) -> None:
        now = time.perf_counter()
        if self._train_start is not None:          # the update phase that just finished
            self.logger.record("perf/train_s", now - self._train_start)
        self._roll_start = now

    def _on_rollout_end(self) -> None:
        now = time.perf_counter()
        self.logger.record("perf/rollout_s", now - self._roll_start)

        dts = self.num_timesteps - self._last_ts
        if self._last_end is not None and now > self._last_end:
            self.logger.record("perf/fps_rolling", dts / (now - self._last_end))  # steps / full iteration
        self._last_ts, self._last_end = self.num_timesteps, now

        base = self._base_env()
        n = getattr(base, "_perf_env_n", 0)
        if n:
            self.logger.record("perf/env_step_ms", getattr(base, "_perf_env_s", 0.0) / n * 1000.0)
            base._perf_env_s, base._perf_env_n = 0.0, 0

        self.logger.record("perf/elapsed_min", (now - self._t0) / 60.0)
        rss = _rss_gb()
        if rss is not None:
            self.logger.record("perf/rss_gb", rss)
        if torch.cuda.is_available():
            self.logger.record("perf/gpu_mem_gb", torch.cuda.memory_allocated() / 1e9)

        self._train_start = now                     # the PPO update starts now

    def _on_step(self) -> bool:
        return True
