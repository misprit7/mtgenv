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
from stable_baselines3.common.callbacks import BaseCallback
from stable_baselines3.common.vec_env.base_vec_env import VecEnv

from .env import MtgEnv
from .inference import BatchedPolicy

_U64 = (1 << 64) - 1

# Obs `globals` indices used by the potential function (must track obs.rs::encode_globals): the
# per-seat block is [life, poison, hand, library, graveyard, exile, battlefield, mana×6], me first.
_G_MY_LIFE, _G_MY_HAND, _G_MY_BF = 16, 18, 22
_G_OPP_LIFE, _G_OPP_HAND, _G_OPP_BF = 29, 31, 35


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

    def __init__(self, deck, pool_dir, num_envs, p_random=0.2, seed=0, max_decisions=3000,
                 device="cpu", shaping_coef=0.0, gamma=0.999):
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
        # Potential-based reward shaping (GYM_PLAN §5): F = γΦ(s') − Φ(s) added to the sparse ±1.
        # `shaping_coef` scales it and is annealed to 0 by `ShapingAnneal` so the final policy
        # optimizes only the true terminal reward (PBRS is policy-invariant; the anneal is belt-and-
        # suspenders + removes any Φ-approximation artifacts late in training).
        self.shaping_coef = float(shaping_coef)
        self.gamma = float(gamma)
        self._prev_phi = np.zeros(num_envs, dtype=np.float32)

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

    @staticmethod
    def _phi_batch(obs):
        """Potential Φ(s) per env (shape ``(N,)``), from the learner's perspective, bounded in
        ~[−1, 1]: a small tanh-squashed mix of life, board-power, and card-count differentials.
        Each ``tanh`` keeps any one term from dominating; weights sum to 1. (Magician's caution:
        a life-only baseline is weak — board + cards matter — but Φ is only a learning crutch.)"""
        g = obs["globals"]                                            # (N, G)
        dlife = g[:, _G_MY_LIFE] - g[:, _G_OPP_LIFE]
        dcards = (g[:, _G_MY_HAND] + g[:, _G_MY_BF]) - (g[:, _G_OPP_HAND] + g[:, _G_OPP_BF])
        bf = obs["bf_feat"]                                           # (N, MAX_PERM, F_PERM)
        present = bf[:, :, 0] > 0.5
        mine = present & (bf[:, :, 1] > 0.5)
        dpower = (bf[:, :, 2] * mine).sum(1) - (bf[:, :, 2] * (present & ~mine)).sum(1)
        phi = (0.5 * np.tanh(dlife / 10.0)
               + 0.3 * np.tanh(dpower / 6.0)
               + 0.2 * np.tanh(dcards / 4.0))
        return phi.astype(np.float32)

    def _apply_shaping(self, obs, rewards, dones):
        """Add F = γΦ(s') − Φ(s) to ``rewards`` (Φ(terminal)≜0). On a done env, ``obs`` already holds
        the *reset* episode's first learner state, so its Φ becomes the next step's baseline and the
        terminal transition uses only −Φ(s) (no leakage across the episode boundary)."""
        if self.shaping_coef == 0.0:
            self._prev_phi = self._phi_batch(obs)  # keep baseline fresh so toggling on is clean
            return rewards
        phi_next = self._phi_batch(obs)
        f = np.where(dones, -self._prev_phi, self.gamma * phi_next - self._prev_phi)
        self._prev_phi = phi_next
        return rewards + self.shaping_coef * f.astype(np.float32)

    # ── VecEnv API ──────────────────────────────────────────────────────────────────────────────
    def reset(self):
        for i, env in enumerate(self.envs):
            self._opp.assign(i)
            env.ext_reset(self._next_seed())
        dummy_r = np.zeros(self.num_envs, dtype=np.float32)
        dummy_d = np.zeros(self.num_envs, dtype=bool)
        self._pump(dummy_r, dummy_d, [{} for _ in range(self.num_envs)], record_terminals=False)
        obs = self._collect_obs()
        self._prev_phi = self._phi_batch(obs)  # episode-start baseline; no shaping reward on reset
        return obs

    def step_async(self, actions):
        self._actions = np.asarray(actions, dtype=np.int64).reshape(-1)

    def step_wait(self):
        rewards = np.zeros(self.num_envs, dtype=np.float32)
        dones = np.zeros(self.num_envs, dtype=bool)
        infos = [{} for _ in range(self.num_envs)]
        for i, env in enumerate(self.envs):
            env.ext_apply(int(self._actions[i]))
            # Capture the learner decision's semantic record NOW, before _pump answers opponent
            # decisions (which would overwrite it). Non-empty only on a finalizing sub-step (#68).
            rec = env.ext_take_stats()
            if rec:
                infos[i]["decision_stats"] = rec
        self._pump(rewards, dones, infos, record_terminals=True)
        obs = self._collect_obs()
        rewards = self._apply_shaping(obs, rewards, dones)
        return obs, rewards, dones, infos

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


class ShapingAnneal(BaseCallback):
    """Linearly anneal the vec env's potential-based shaping coefficient from ``coef0`` to 0 over the
    first ``anneal_frac`` of training (GYM_PLAN §5). After that the policy trains on the pure ±1
    terminal reward. Locates the ``BatchedSelfPlayVecEnv`` under any ``VecEnvWrapper`` layers."""

    def __init__(self, total_timesteps, coef0=0.5, anneal_frac=0.6, verbose=0):
        super().__init__(verbose)
        self.total = max(int(total_timesteps), 1)
        self.coef0 = float(coef0)
        self.anneal_frac = max(float(anneal_frac), 1e-6)

    def _base_env(self):
        env = self.training_env
        while hasattr(env, "venv"):
            env = env.venv
        return env

    def _coef(self):
        frac = self.num_timesteps / (self.total * self.anneal_frac)
        return self.coef0 * max(0.0, 1.0 - frac)

    def _on_training_start(self) -> None:
        env = self._base_env()
        if hasattr(env, "shaping_coef"):
            env.shaping_coef = self.coef0

    def _on_step(self) -> bool:
        env = self._base_env()
        if hasattr(env, "shaping_coef"):
            env.shaping_coef = self._coef()
            self.logger.record("train/shaping_coef", env.shaping_coef)
        return True
