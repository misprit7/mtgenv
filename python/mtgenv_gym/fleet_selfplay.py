"""Fleet-backed self-play vec env (M3.4). Same self-play regime as `BatchedSelfPlayVecEnv` — each
game's opponent is a recency-biased frozen pool checkpoint with a `p_random` floor, opponent
inference batched per checkpoint — but the **stepping** runs in `mtg_py.Fleet` worker threads instead
of a single-threaded Python `_pump`. The env owns one `Fleet` (all N games on worker threads); a step
applies the learner's factored action, then pumps: parallel-step (Rust), answer opponent-pending
decisions (Python forwards, grouped by checkpoint), auto-reset terminals, until every game sits at a
learner decision again. Obs/masks cross once per pump round as bytes (`np.frombuffer`).

Drop-in for `MaskablePPO` like the batched env: provides per-env masks via `env_method`. Opponent
routing reuses `_PooledBatchedOpponent` unchanged through a tiny `_FleetEnvView` (ext_obs/ext_mask
read the decoded batch row).

Status (M3.4, shipped at ~2.8x over the pump at 512 envs, byte-identical behavior): the fleet is the
default self-play vec env. It is NOT yet fully forward-bound (GPU ~44% peak). Profiling `step_wait`
(256 envs) put `Fleet.advance` at 63% of the time — ~11 pump rounds per step (one per opponent
sub-step between learner decisions), each a worker round-trip + a main-side O(N) re-assemble. The
decode (14%) is already vectorized; an incremental-encode attempt (re-encode only changed rows) was
measured **neutral-to-worse** (the per-round clone offset the savings) and reverted. DEFERRED levers
toward fully-GPU-bound, in the order the profile suggests, for when throughput binds again (bigger
pools / longer runs):
  (a) incremental ASSEMBLE — have each `Fleet.advance` patch only the changed rows into a persistent
      main buffer instead of re-stitching all N rows every round (kills the O(N)-per-round copy).
  (b) fewer pump ROUNDS — most rounds only touch a few opponent-pending envs; collapse consecutive
      opponent sub-steps of the same game where the codec allows, or overlap the learner forward with
      the next round's stepping.
  (c) bigger n_envs — now that envs are cheap, larger batches make the forward the bottleneck (the
      GPU-batch-size story) rather than the pump.
"""

from __future__ import annotations

import numpy as np
from stable_baselines3.common.vec_env.base_vec_env import VecEnv

import mtg_py

from .batched_selfplay import _PooledBatchedOpponent, _G_MY_LIFE, _G_OPP_LIFE, _G_MY_HAND, _G_OPP_HAND, _G_MY_BF, _G_OPP_BF

_U64 = (1 << 64) - 1
_SPEC = mtg_py.PyGame.obs_spec()  # [(name, rows, cols, is_int), ...]
_ACTION_DIM = mtg_py.PyGame.action_dim()
_AGENT_SEAT = 0


class _FleetEnvView:
    """Adapts one env's decoded batch row to the `ext_obs`/`ext_mask` interface `_PooledBatchedOpponent`
    expects — so the opponent pool + BatchedPolicy are reused verbatim."""

    __slots__ = ("_obs", "_mask")

    def __init__(self, obs_row, mask_row):
        self._obs = obs_row
        self._mask = mask_row

    def ext_obs(self):
        return self._obs

    def ext_mask(self):
        return self._mask


class FleetSelfPlayVecEnv(VecEnv):
    """N self-play games stepped in `mtg_py.Fleet` worker threads; opponent inference batched across
    games. Same observation/action spaces as `MtgEnv` (built from `PyGame.obs_spec`)."""

    def __init__(self, deck, pool_dir, num_envs, num_workers=8, p_random=0.2, seed=0,
                 shaping_coef=0.0, gamma=0.999, device="cpu"):
        self.deck = deck
        self.num_workers = num_workers
        # Reuse MtgEnv's spaces so MaskablePPO/EntityExtractor are unchanged.
        from .env import MtgEnv

        probe = MtgEnv(deck=deck)
        observation_space, action_space = probe.observation_space, probe.action_space
        self.action_dim = probe.action_dim
        # Reuse the probe's deck-determined card-id one-hot index (same categories as MtgEnv._to_obs).
        self._cardid_index = probe._cardid_index
        self._cardid_dim = probe._cardid_dim
        self._cardid_unknown = probe._cardid_unknown
        self._cardid_tables = probe._cardid_tables
        # grp_id → category LUT (vectorizes the one-hot decode — no per-row Python loop). Ids past the
        # LUT (a rare unseen token) or absent map to the "unknown" slot; id 0 (empty row) → no one-hot.
        maxg = max(self._cardid_index) if self._cardid_index else 0
        self._id2cat = np.full(maxg + 1, self._cardid_unknown, dtype=np.int64)
        for g, i in self._cardid_index.items():
            self._id2cat[g] = i
        super().__init__(num_envs, observation_space, action_space)
        self._opp = _PooledBatchedOpponent(pool_dir, p_random=p_random, rng_seed=seed, device=device)
        self._seed = (int(seed) * 2862933555777941757 + 3037000493) & _U64
        self._seedctr = 0
        self._actions = np.zeros(num_envs, dtype=np.int64)
        self._masks = np.zeros((num_envs, self.action_dim), dtype=bool)
        # Potential-based shaping (mirrors BatchedSelfPlayVecEnv).
        self.shaping_coef = float(shaping_coef)
        self.gamma = float(gamma)
        self._prev_phi = np.zeros(num_envs, dtype=np.float32)
        self._fleet = None

    def _next_seed(self):
        self._seed = (self._seed * 6364136223846793005 + 1442695040888963407) & _U64
        return self._seed

    def _fresh_seed(self):
        self._seedctr += 1
        return self._seedctr

    # ── batch decode ─────────────────────────────────────────────────────────────────────────────
    def _decode(self):
        """Decode the fleet's current tick into the batched Dict obs (matching MtgEnv._to_obs: rows==1
        squeezed to (n, cols); per-table `*_cardid` one-hots from `*_ids`) + seat/terminal/mask."""
        t = self._fleet.tick()
        n = self.num_envs
        obs = {}
        for name, rows, cols, is_int in _SPEC:
            arr = np.frombuffer(t[name], dtype=np.int64 if is_int else np.float32)
            obs[name] = arr.reshape((n, cols) if rows == 1 else (n, rows, cols)).copy()
        lut = self._id2cat
        for tbl, R in self._cardid_tables.items():
            ids = obs[f"{tbl}_ids"].ravel()  # (n*R,)
            cats = np.where(ids < len(lut), lut[np.clip(ids, 0, len(lut) - 1)], self._cardid_unknown)
            oh = np.zeros((ids.size, self._cardid_dim), dtype=np.float32)
            present = np.flatnonzero(ids != 0)  # id 0 = empty row → all-zero one-hot
            oh[present, cats[present]] = 1.0
            obs[f"{tbl}_cardid"] = oh.reshape(n, R, self._cardid_dim)
        mask = np.frombuffer(t["mask"], dtype=np.uint8).reshape(n, self.action_dim).astype(bool).copy()
        seat = np.frombuffer(t["seat"], dtype=np.int32).copy()
        terminal = np.frombuffer(t["terminal"], dtype=np.int32).astype(bool).copy()
        return obs, mask, seat, terminal

    def _learner_obs(self, obs):
        return obs

    # ── the fleet pump ───────────────────────────────────────────────────────────────────────────
    def _pump(self, rewards, dones, infos, record_terminals):
        """Advance until every game sits at a learner (seat-0) decision: answer opponent decisions
        (grouped by checkpoint) and auto-reset terminals, in fleet.advance rounds."""
        for _ in range(100_000):
            obs, mask, seat, terminal = self._decode()
            step_envs, step_acts, reset_envs, reset_seeds = [], [], [], []
            pending = []  # (env_index, view) for opponent decisions
            for i in range(self.num_envs):
                if terminal[i]:
                    if record_terminals and not dones[i]:
                        s = self._fleet.summary(i)  # (winner, turns, reason)
                        rewards[i] = _reward(s)
                        dones[i] = True
                        infos[i]["terminal_observation"] = {k: v[i] for k, v in obs.items()}
                        if s is not None:
                            infos[i]["episode_summary"] = {"winner": s[0], "turns": s[1], "reason": s[2]}
                    # (re)assign this env's opponent for the fresh game + queue a reset.
                    self._opp.assign(i)
                    reset_envs.append(i)
                    reset_seeds.append(self._fresh_seed())
                elif seat[i] != _AGENT_SEAT:
                    pending.append((i, _FleetEnvView({k: v[i] for k, v in obs.items()}, mask[i])))
            if pending:
                for i, a in self._opp.resolve(pending, self._opp._rng).items():
                    step_envs.append(i)
                    step_acts.append(int(a))
            if not step_envs and not reset_envs:
                return  # every game at a learner decision
            self._fleet.advance(step_envs, step_acts, reset_envs, reset_seeds)
        raise RuntimeError("fleet self-play pump did not converge to learner decisions")

    def _phi(self, obs):
        g = obs["globals"]  # (N, G) — rows==1 squeezed
        dlife = g[:, _G_MY_LIFE] - g[:, _G_OPP_LIFE]
        dcards = (g[:, _G_MY_HAND] + g[:, _G_MY_BF]) - (g[:, _G_OPP_HAND] + g[:, _G_OPP_BF])
        bf = obs["bf_feat"]
        present = bf[:, :, 0] > 0.5
        mine = present & (bf[:, :, 1] > 0.5)
        dpower = (bf[:, :, 2] * mine).sum(1) - (bf[:, :, 2] * (present & ~mine)).sum(1)
        return (0.5 * np.tanh(dlife / 10.0) + 0.3 * np.tanh(dpower / 6.0) + 0.2 * np.tanh(dcards / 4.0)).astype(np.float32)

    def _apply_shaping(self, obs, rewards, dones):
        if self.shaping_coef == 0.0:
            self._prev_phi = self._phi(obs)
            return rewards
        phi_next = self._phi(obs)
        f = np.where(dones, -self._prev_phi, self.gamma * phi_next - self._prev_phi)
        self._prev_phi = phi_next
        return rewards + self.shaping_coef * f.astype(np.float32)

    # ── VecEnv API ───────────────────────────────────────────────────────────────────────────────
    def reset(self):
        # Fresh fleet (drops old worker threads); assign each env's opponent, pump to learner.
        self._seedctr = int(self._next_seed() % 1_000_000)
        self._fleet = mtg_py.Fleet(self.deck, self.num_envs, self.num_workers, True, self._seedctr)
        self._seedctr += self.num_envs
        for i in range(self.num_envs):
            self._opp.assign(i)
        self._pump(np.zeros(self.num_envs), np.zeros(self.num_envs, bool),
                   [{} for _ in range(self.num_envs)], record_terminals=False)
        obs, mask, _seat, _term = self._decode()
        self._masks = mask
        self._prev_phi = self._phi(obs)
        return self._learner_obs(obs)

    def step_async(self, actions):
        self._actions = np.asarray(actions, dtype=np.int64).reshape(-1)

    def step_wait(self):
        rewards = np.zeros(self.num_envs, dtype=np.float32)
        dones = np.zeros(self.num_envs, dtype=bool)
        infos = [{} for _ in range(self.num_envs)]
        # Apply the learner's factored action to every (learner-pending) env, then pump.
        all_envs = list(range(self.num_envs))
        self._fleet.advance(all_envs, [int(a) for a in self._actions], [], [])
        self._pump(rewards, dones, infos, record_terminals=True)
        obs, mask, _seat, _term = self._decode()
        self._masks = mask
        rewards = self._apply_shaping(obs, rewards, dones)
        return self._learner_obs(obs), rewards, dones, infos

    def close(self):
        self._fleet = None

    def env_method(self, method_name, *args, indices=None, **kwargs):
        if method_name in ("action_masks", "action_mask"):
            return [self._masks[i].copy() for i in self._idx(indices)]
        raise NotImplementedError(method_name)

    def get_attr(self, attr_name, indices=None):
        return [getattr(self, attr_name, None) for _ in self._idx(indices)]

    def set_attr(self, attr_name, value, indices=None):
        setattr(self, attr_name, value)

    def env_is_wrapped(self, wrapper_class, indices=None):
        return [False for _ in self._idx(indices)]

    def _idx(self, indices):
        if indices is None:
            return list(range(self.num_envs))
        if isinstance(indices, int):
            return [indices]
        return list(indices)


def _reward(summary):
    """Terminal reward from the learner's (seat 0) perspective: +1 win / -1 loss / 0 draw."""
    if summary is None:
        return 0.0
    winner = summary[0]
    if winner is None:
        return 0.0
    return 1.0 if winner == _AGENT_SEAT else -1.0
