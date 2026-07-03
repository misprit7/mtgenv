"""LightZero env adapter over ``mtgenv_gym.MtgEnv`` for the swine matchup (M1).

This wraps the existing single-agent Gym env (which already answers the opponent's decisions
internally and surfaces only the learner's factored sub-decisions) as a LightZero ``BaseEnv``:

  * observation  -> flattened float32 vector (the Dict obs concatenated in a fixed key order,
                    card-id one-hots included), shape ``(obs_dim,)``.
  * action_mask  -> int8 length ``action_dim`` (the engine's per-sub-step legality mask).
  * to_play      -> ``-1``: we model the game as a **single-agent stochastic MDP** (opponent +
                    card draws folded into env stochasticity). See README "why single-agent".

Chance is handled by Stochastic MuZero's learned VQ ``ChanceEncoder``
(``use_ture_chance_label_in_chance_encoder=False``), so no ground-truth ``chance`` label is
emitted here — the obs dict carries only ``{observation, action_mask, to_play}``.

Registered as ``mtg_swine`` so a LightZero config can reference it by
``env=dict(type='mtg_swine', import_names=['swine_lightzero_env'])``.
"""

from __future__ import annotations

import copy
from typing import List

import numpy as np
import gymnasium as gym
from gymnasium import spaces

from ding.envs import BaseEnv, BaseEnvTimestep
from ding.utils import ENV_REGISTRY
from ding.torch_utils import to_ndarray

from mtgenv_gym import MtgEnv

# obs["globals"] indices (match python/mtgenv_gym/batched_selfplay.py — the PPO training's own layout).
_G_MY_LIFE, _G_MY_HAND, _G_MY_BF = 16, 18, 22
_G_OPP_LIFE, _G_OPP_HAND, _G_OPP_BF = 29, 31, 35


@ENV_REGISTRY.register('mtg_swine')
class MtgSwineEnv(BaseEnv):
    """Single-agent LightZero view of ``MtgEnv`` (learner seat vs an internal opponent)."""

    config = dict(
        env_id='mtg_swine',
        # (str) Matchup deck understood by mtg_py.PyGame (e.g. 'swine', 'bears', 'demo').
        deck='swine',
        # (str) Opponent policy the wrapped MtgEnv answers with internally. 'random' = random-legal
        # (matches the eval baseline). A frozen-self opponent is wired at M3 (self-play).
        opponent='random',
        # (int) Safety cap on the learner's factored sub-decisions before truncation.
        max_decisions=3000,
        # (int) Fixed seat the learner plays (reward is from this seat's perspective).
        agent_seat=0,
        # (float) Potential-based reward-shaping coefficient (0.0 = OFF = pure sparse ±1 env, the
        # default). >0 adds F = gamma*Phi(s') - Phi(s) with the SAME card-dominant Phi the PPO
        # training uses (batched_selfplay._phi_batch): 0.5*tanh(dcards/4)+0.3*tanh(dpower/6)+
        # 0.2*tanh(dlife/10). Policy-invariant (Phi(terminal)=0); a cold-start crutch for MuZero's
        # value net (eval is always the raw ±1). See README "M3 cold-start".
        reward_shaping=0.0,
        # (float) gamma used in the shaping term; match the policy discount_factor.
        shaping_gamma=0.997,
    )

    @classmethod
    def default_config(cls: type):
        from easydict import EasyDict

        cfg = EasyDict(copy.deepcopy(cls.config))
        cfg.cfg_type = cls.__name__ + 'Dict'
        return cfg

    def __init__(self, cfg: dict) -> None:
        self._cfg = cfg
        self._init_flag = False
        self._deck = cfg.get('deck', 'swine')
        self._opponent = cfg.get('opponent', 'random')
        self._max_decisions = int(cfg.get('max_decisions', 3000))
        self._agent_seat = int(cfg.get('agent_seat', 0))
        self._shaping = float(cfg.get('reward_shaping', 0.0))
        self._shaping_gamma = float(cfg.get('shaping_gamma', 0.997))
        self._prev_phi = 0.0

        # Build one env up front to read the flat obs dimension + a stable key order for flattening
        # (Python dict insertion order is deterministic, so the concat order is fixed by these keys).
        probe = MtgEnv(deck=self._deck, opponent=self._opponent,
                       agent_seat=self._agent_seat, max_decisions=self._max_decisions)
        obs0, _ = probe.reset(seed=0)
        self._obs_keys = list(obs0.keys())
        self._obs_dim = int(sum(np.asarray(obs0[k]).size for k in self._obs_keys))
        self._action_dim = int(probe.action_dim)
        del probe

        self._observation_space = spaces.Box(
            low=-np.inf, high=np.inf, shape=(self._obs_dim,), dtype=np.float32
        )
        self._action_space = spaces.Discrete(self._action_dim)
        self._reward_space = spaces.Box(low=-1.0, high=1.0, shape=(1,), dtype=np.float32)

        self._env: MtgEnv | None = None
        self._seed = None
        self._dynamic_seed = True
        self._final_eval_reward = 0.0
        self._mask = np.zeros(self._action_dim, dtype=np.int8)

    # ── obs assembly ────────────────────────────────────────────────────────────────────────
    def _flatten(self, obs_dict) -> np.ndarray:
        return np.concatenate(
            [np.asarray(obs_dict[k], dtype=np.float32).ravel() for k in self._obs_keys]
        ).astype(np.float32)

    def _phi(self, obs_dict) -> float:
        """Single-env potential (card-dominant), mirroring batched_selfplay._phi_batch."""
        g = np.asarray(obs_dict["globals"], dtype=np.float32)
        dlife = g[_G_MY_LIFE] - g[_G_OPP_LIFE]
        dcards = (g[_G_MY_HAND] + g[_G_MY_BF]) - (g[_G_OPP_HAND] + g[_G_OPP_BF])
        bf = np.asarray(obs_dict["bf_feat"], dtype=np.float32)
        present = bf[:, 0] > 0.5
        mine = present & (bf[:, 1] > 0.5)
        dpower = (bf[:, 2] * mine).sum() - (bf[:, 2] * (present & ~mine)).sum()
        return float(0.5 * np.tanh(dcards / 4.0)
                     + 0.3 * np.tanh(dpower / 6.0)
                     + 0.2 * np.tanh(dlife / 10.0))

    def _lz_obs(self, obs_dict, mask) -> dict:
        self._mask = np.asarray(mask, dtype=np.int8)
        return {
            'observation': self._flatten(obs_dict),
            'action_mask': self._mask.copy(),
            'to_play': -1,
            # LightZero's collector/evaluator read a `timestep` from the obs (temporal models). It is
            # unused by Stochastic MuZero; -1 matches the framework's own default (silences a warning).
            'timestep': -1,
        }

    # ── BaseEnv API ─────────────────────────────────────────────────────────────────────────
    def reset(self):
        if not self._init_flag:
            self._env = MtgEnv(deck=self._deck, opponent=self._opponent,
                               agent_seat=self._agent_seat, max_decisions=self._max_decisions)
            self._init_flag = True

        # ding seed convention: fixed seed unless dynamic, in which case perturb per-episode.
        if self._seed is not None and self._dynamic_seed:
            np_seed = 100 * np.random.randint(1, 1000)
            ep_seed = (self._seed + np_seed) & ((1 << 63) - 1)
        elif self._seed is not None:
            ep_seed = self._seed
        else:
            ep_seed = int(np.random.randint(0, 1 << 31))

        obs_dict, info = self._env.reset(seed=ep_seed)
        self._final_eval_reward = 0.0
        if self._shaping:
            self._prev_phi = self._phi(obs_dict)   # baseline for the first step's shaping term
        return self._lz_obs(obs_dict, info['action_mask'])

    def step(self, action):
        obs_dict, reward, terminated, truncated, info = self._env.step(int(action))
        done = bool(terminated or truncated)
        # eval_episode_return stays RAW (±1) — the win/loss the evaluator reports is never shaped.
        self._final_eval_reward += float(reward)

        train_reward = float(reward)
        if self._shaping:
            # F = gamma*Phi(s') - Phi(s); Phi(terminal) := 0 so the terminal transition uses -Phi(s).
            if done:
                f = -self._prev_phi
            else:
                phi_next = self._phi(obs_dict)
                f = self._shaping_gamma * phi_next - self._prev_phi
                self._prev_phi = phi_next
            train_reward += self._shaping * f

        lz_obs = self._lz_obs(obs_dict, info['action_mask'])
        # Reward must be a 0-d scalar array (not shape (1,)): the buffer pads with np.array(0.) and
        # np.asarray of mixed (1,)+() shapes is inhomogeneous -> crash in _compute_target_reward_value.
        rew = to_ndarray(train_reward).astype(np.float32)
        out_info = {}
        if done:
            out_info['eval_episode_return'] = self._final_eval_reward
        return BaseEnvTimestep(lz_obs, rew, done, out_info)

    def close(self) -> None:
        self._init_flag = False
        self._env = None

    def seed(self, seed: int, dynamic_seed: bool = True) -> None:
        self._seed = int(seed)
        self._dynamic_seed = dynamic_seed
        np.random.seed(self._seed)

    @property
    def legal_actions(self) -> List[int]:
        return np.nonzero(self._mask)[0].tolist()

    def random_action(self) -> np.ndarray:
        legal = self.legal_actions
        return np.array([int(np.random.choice(legal))], dtype=np.int64)

    @property
    def observation_space(self) -> gym.spaces.Space:
        return self._observation_space

    @property
    def action_space(self) -> gym.spaces.Space:
        return self._action_space

    @property
    def reward_space(self) -> gym.spaces.Space:
        return self._reward_space

    @staticmethod
    def create_collector_env_cfg(cfg: dict) -> List[dict]:
        collector_env_num = cfg.pop('collector_env_num')
        return [copy.deepcopy(cfg) for _ in range(collector_env_num)]

    @staticmethod
    def create_evaluator_env_cfg(cfg: dict) -> List[dict]:
        evaluator_env_num = cfg.pop('evaluator_env_num')
        cfg = copy.deepcopy(cfg)
        return [copy.deepcopy(cfg) for _ in range(evaluator_env_num)]

    def __repr__(self) -> str:
        return f"MtgSwineEnv(deck={self._deck}, opponent={self._opponent}, obs_dim={self._obs_dim})"
