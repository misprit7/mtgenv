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
        return self._lz_obs(obs_dict, info['action_mask'])

    def step(self, action):
        obs_dict, reward, terminated, truncated, info = self._env.step(int(action))
        done = bool(terminated or truncated)
        self._final_eval_reward += float(reward)

        lz_obs = self._lz_obs(obs_dict, info['action_mask'])
        # Reward must be a 0-d scalar array (not shape (1,)): the buffer pads with np.array(0.) and
        # np.asarray of mixed (1,)+() shapes is inhomogeneous -> crash in _compute_target_reward_value.
        rew = to_ndarray(float(reward)).astype(np.float32)
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
