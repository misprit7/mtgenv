"""Batched self-play vectorized env (GYM_PLAN §6, task #41) — the bottleneck fix.

The old self-play vec env (`DummyVecEnv` of `MtgEnv(opponent=OpponentPool)`) answered every opponent
decision with a **synchronous, single-sample** `policy.predict` *inside* each env's `step`. A
1-sample forward costs ~the same as a 64-sample one (launch overhead; see `bench_infer.py`), so N
envs paid ~N× the necessary inference cost.

`BatchedSelfPlayVecEnv` keeps the **same self-play regime** (each game's opponent is a frozen pool
checkpoint, recency-biased, with a `p_random` floor — identical to `OpponentPool`) but steps all N
games in **lockstep** and collects *every* pending opponent decision across games into **one**
`BatchedPolicy.act` forward (grouped by which checkpoint serves it). It is single-threaded and
deterministic — no queues, no GIL games — and the "advance every game to a decision, answer the
whole batch at once" structure is exactly what batched MCTS will reuse for leaf evaluation.

Drop-in for SB3: it provides per-env action masks via `env_method("action_masks")`, so it plugs
straight into `MaskablePPO` with no `ActionMasker` wrapper.
"""

from __future__ import annotations

import glob
import os

import numpy as np
from stable_baselines3.common.vec_env.base_vec_env import VecEnv

from .env import MtgEnv
from .inference import BatchedPolicy

_U64 = (1 << 64) - 1


class _PooledBatchedOpponent:
    """Per-episode opponent assignment + batched resolution, mirroring `league.OpponentPool` but
    routing inference through `BatchedPolicy` so decisions for the *same* checkpoint batch together.

    `assign(i)` draws env *i*'s opponent for the coming episode: `None` (a random-legal opponent,
    with probability `p_random` or when the pool is empty) or a checkpoint path (recency-biased).
    `resolve(pending, rng)` answers a list of `(env_index, obs, mask)` opponent decisions, grouping
    by assignment so each distinct checkpoint runs a single forward over its whole group.
    """

    def __init__(self, pool_dir, p_random=0.2, rng_seed=0, device="cpu"):
        self.pool_dir = pool_dir
        self.p_random = p_random
        self.device = device
        self._rng = np.random.default_rng(rng_seed)
        self._cache = {}     # checkpoint path -> BatchedPolicy
        self._assign = {}    # env_index -> None | checkpoint path

    def _checkpoints(self):
        if not os.path.isdir(self.pool_dir):
            return []
        return sorted(glob.glob(os.path.join(self.pool_dir, "*.zip")))

    def assign(self, i):
        ck = self._checkpoints()
        if len(self._cache) > len(ck):  # drop policies whose checkpoint was pruned (bounds memory)
            live = set(ck)
            for stale in [p for p in self._cache if p not in live]:
                self._cache.pop(stale, None)
        if not ck or self._rng.random() < self.p_random:
            self._assign[i] = None
            return
        w = np.arange(1, len(ck) + 1, dtype=float)
        w /= w.sum()
        self._assign[i] = ck[int(self._rng.choice(len(ck), p=w))]

    def _policy(self, path):
        if path not in self._cache:
            from sb3_contrib import MaskablePPO
            from mtgenv_gym.policy import EntityExtractor  # noqa: F401 — needed to unpickle extractor

            try:
                self._cache[path] = BatchedPolicy(MaskablePPO.load(path, device=self.device))
            except Exception:
                self._cache[path] = None  # half-written / unreadable → treated as random below
        return self._cache[path]

    def resolve(self, pending, rng):
        """`pending`: list of `(env_index, env)`. Returns `{env_index: action}`. Observations are
        encoded lazily *inside the model groups only* — a random-opponent env needs just its mask,
        so we skip the (non-trivial) obs encoding for it."""
        groups = {}
        for (i, env) in pending:
            groups.setdefault(self._assign.get(i), []).append((i, env))
        out = {}
        for handle, items in groups.items():
            bp = self._policy(handle) if handle is not None else None
            if bp is None:  # random opponent (or unreadable checkpoint) — mask only
                for (i, env) in items:
                    out[i] = int(rng.choice(np.flatnonzero(env.ext_mask())))
                continue
            acts = bp.act([env.ext_obs() for (_, env) in items],
                          [env.ext_mask() for (_, env) in items], deterministic=False)
            for k, (i, _env) in enumerate(items):
                out[i] = int(acts[k])
        return out


class BatchedSelfPlayVecEnv(VecEnv):
    """N self-play `MtgEnv` games stepped in lockstep, opponent inference batched across games."""

    def __init__(self, deck, pool_dir, num_envs, p_random=0.2, seed=0, max_decisions=200_000,
                 device="cpu"):
        self.deck = deck
        self.envs = [
            MtgEnv(deck=deck, opponent="external", max_decisions=max_decisions)
            for _ in range(num_envs)
        ]
        super().__init__(num_envs, self.envs[0].observation_space, self.envs[0].action_space)
        self.action_dim = self.envs[0].action_dim
        self._opp = _PooledBatchedOpponent(pool_dir, p_random=p_random, rng_seed=seed, device=device)
        self._masks = np.zeros((num_envs, self.action_dim), dtype=bool)
        self._actions = np.zeros(num_envs, dtype=np.int64)
        self._seed = (int(seed) * 2862933555777941757 + 3037000493) & _U64

    # ── seeding ───────────────────────────────────────────────────────────────────────────────
    def _next_seed(self):
        self._seed = (self._seed * 6364136223846793005 + 1442695040888963407) & _U64
        return self._seed

    # ── the lockstep pump ───────────────────────────────────────────────────────────────────────
    def _pump(self, rewards, dones, infos, record_terminals):
        """Advance every env until it sits at a *learner* decision. Opponent decisions across all
        envs are gathered and answered in batched rounds; terminals are recorded (once) and the env
        auto-reset, whose fresh game is pumped in the same loop."""
        for _ in range(100_000):  # guard against a pathological non-terminating loop
            pending = []
            for i, env in enumerate(self.envs):
                st = env.ext_state()
                if st == "terminal":
                    if record_terminals and not dones[i]:
                        rewards[i] = env.ext_reward()
                        dones[i] = True
                        infos[i]["terminal_observation"] = env.ext_last_learner_obs()
                        if env.ext_truncated():
                            infos[i]["TimeLimit.truncated"] = True
                        s = env.summary()
                        if s is not None:
                            infos[i]["episode_summary"] = s
                    self._opp.assign(i)
                    env.ext_reset(self._next_seed())
                    st = env.ext_state()
                if st == "opponent":
                    pending.append((i, env))
            if not pending:
                return
            for i, action in self._opp.resolve(pending, self._opp._rng).items():
                self.envs[i].ext_apply(action)
        raise RuntimeError("self-play pump did not converge to learner decisions")

    def _collect_obs(self):
        rows = []
        for i, env in enumerate(self.envs):
            assert env.ext_state() == "learner", f"env {i} at {env.ext_state()}, expected learner"
            rows.append(env.ext_obs())
            self._masks[i] = env.ext_mask()
        return {k: np.stack([r[k] for r in rows]) for k in rows[0]}

    # ── VecEnv API ──────────────────────────────────────────────────────────────────────────────
    def reset(self):
        for i, env in enumerate(self.envs):
            self._opp.assign(i)
            env.ext_reset(self._next_seed())
        dummy_r = np.zeros(self.num_envs, dtype=np.float32)
        dummy_d = np.zeros(self.num_envs, dtype=bool)
        self._pump(dummy_r, dummy_d, [{} for _ in range(self.num_envs)], record_terminals=False)
        return self._collect_obs()

    def step_async(self, actions):
        self._actions = np.asarray(actions, dtype=np.int64).reshape(-1)

    def step_wait(self):
        rewards = np.zeros(self.num_envs, dtype=np.float32)
        dones = np.zeros(self.num_envs, dtype=bool)
        infos = [{} for _ in range(self.num_envs)]
        for i, env in enumerate(self.envs):
            env.ext_apply(int(self._actions[i]))
        self._pump(rewards, dones, infos, record_terminals=True)
        return self._collect_obs(), rewards, dones, infos

    def close(self):
        for env in self.envs:
            close = getattr(env, "close", None)
            if close is not None:
                close()

    # masks for MaskablePPO (no ActionMasker wrapper needed) + the misc VecEnv plumbing.
    def env_method(self, method_name, *args, indices=None, **kwargs):
        if method_name in ("action_masks", "action_mask"):
            return [self._masks[i].copy() for i in self._idx(indices)]
        return [getattr(self.envs[i], method_name)(*args, **kwargs) for i in self._idx(indices)]

    def get_attr(self, attr_name, indices=None):
        return [getattr(self.envs[i], attr_name) for i in self._idx(indices)]

    def set_attr(self, attr_name, value, indices=None):
        for i in self._idx(indices):
            setattr(self.envs[i], attr_name, value)

    def env_is_wrapped(self, wrapper_class, indices=None):
        return [False for _ in self._idx(indices)]

    def _idx(self, indices):
        if indices is None:
            return list(range(self.num_envs))
        if isinstance(indices, int):
            return [indices]
        return list(indices)
