"""Self-play league (GYM_PLAN §8.2): a pool of frozen policy checkpoints used as opponents, plus
the callback that grows it during training.

Coordination is via the filesystem — the training callback writes checkpoints to ``pool_dir`` and
each env's :class:`OpponentPool` re-scans that dir on reset and samples one. That means it works
unchanged across processes (``SubprocVecEnv`` workers, M2c) with no IPC: the workers just read the
same directory the main-process callback writes.

Sampling a *pool* (not only the latest) — biased toward recent but with an ``p_random`` floor —
keeps a diverse opponent set so training doesn't collapse into a narrow rock-paper-scissors cycle.
"""

from __future__ import annotations

import glob
import os

import numpy as np
from stable_baselines3.common.callbacks import BaseCallback


def _load_model(path, device="cpu"):
    # Imported lazily so importing this module doesn't pull torch until a pool is actually used.
    from sb3_contrib import MaskablePPO
    from mtgenv_gym.policy import EntityExtractor  # noqa: F401 — needed to unpickle the extractor

    return MaskablePPO.load(path, device=device)


class ModelOpponent:
    """A fixed-policy opponent backed by one model (a loaded model or a checkpoint path)."""

    def __init__(self, model_or_path, deterministic=False, device="cpu"):
        self.model = _load_model(model_or_path, device) if isinstance(model_or_path, str) else model_or_path
        self.deterministic = deterministic

    def reset(self, rng):  # noqa: D401 - opponent interface
        pass

    def act(self, obs, mask):
        action, _ = self.model.predict(obs, action_masks=mask, deterministic=self.deterministic)
        return int(action)


class OpponentPool:
    """Per-env opponent: each episode draws either a random policy (prob ``p_random``) or a frozen
    checkpoint from ``pool_dir`` (recency-biased). Loaded models are cached by path."""

    def __init__(self, pool_dir, p_random=0.2, recent_bias=True, rng_seed=0, device="cpu"):
        self.pool_dir = pool_dir
        self.p_random = p_random
        self.recent_bias = recent_bias
        self.device = device
        self._cache = {}
        self._current = None  # None ⇒ random opponent this episode
        self._rng = np.random.default_rng(rng_seed)

    def _checkpoints(self):
        if not os.path.isdir(self.pool_dir):
            return []
        return sorted(glob.glob(os.path.join(self.pool_dir, "*.zip")))

    def reset(self, rng=None):
        r = rng if rng is not None else self._rng
        ckpts = self._checkpoints()
        if not ckpts or r.random() < self.p_random:
            self._current = None
            return
        if self.recent_bias and len(ckpts) > 1:
            w = np.arange(1, len(ckpts) + 1, dtype=float)
            w /= w.sum()
            idx = int(r.choice(len(ckpts), p=w))
        else:
            idx = int(r.integers(len(ckpts)))
        self._current = self._get(ckpts[idx])

    def _get(self, path):
        if path not in self._cache:
            try:
                self._cache[path] = _load_model(path, self.device)
            except Exception:
                return None  # half-written / unreadable checkpoint → fall back to random
        return self._cache[path]

    def act(self, obs, mask):
        if self._current is None:
            return int(self._rng.choice(np.flatnonzero(mask)))
        action, _ = self._current.predict(obs, action_masks=mask, deterministic=False)
        return int(action)

    def size(self):
        return len(self._checkpoints())


class PoolCheckpoint(BaseCallback):
    """Snapshot the learning policy into ``pool_dir`` every ``every`` env-steps (atomic write), and
    prune to the newest ``max_pool`` checkpoints (bounds disk + each env's model cache)."""

    def __init__(self, pool_dir, every, n_envs, max_pool=12, verbose=0):
        super().__init__(verbose)
        self.pool_dir = pool_dir
        self.every_calls = max(every // n_envs, 1)
        self.max_pool = max_pool

    def save_now(self, step=None):
        os.makedirs(self.pool_dir, exist_ok=True)
        step = self.num_timesteps if step is None else step
        final = os.path.join(self.pool_dir, f"ckpt_{step:09d}")
        tmp = os.path.join(self.pool_dir, f".tmp_{step:09d}")
        self.model.save(tmp)  # writes tmp.zip
        os.replace(tmp + ".zip", final + ".zip")  # atomic → envs never read a half-written file
        self._prune()
        return final + ".zip"

    def _prune(self):
        ckpts = sorted(glob.glob(os.path.join(self.pool_dir, "ckpt_*.zip")))
        for old in ckpts[: max(0, len(ckpts) - self.max_pool)]:
            try:
                os.remove(old)
            except OSError:
                pass

    def _on_step(self) -> bool:
        if self.n_calls % self.every_calls == 0:
            path = self.save_now()
            if self.verbose:
                print(f"  pool += {os.path.basename(path)}  (size {len(self._checkpoints())})")
        return True

    def _checkpoints(self):
        return glob.glob(os.path.join(self.pool_dir, "ckpt_*.zip"))
