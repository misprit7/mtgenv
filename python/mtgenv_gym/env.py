"""``MtgEnv`` — the milestone-1 Gymnasium environment over ``mtg_py.PyGame`` (GYM_PLAN L4).

Single-agent vs a fixed opponent: the learning policy plays ``agent_seat``; every decision for the
other seat is answered internally by ``opponent`` (a random legal policy by default — a frozen
checkpoint can be plugged in for M2 self-play) and is **not** surfaced as a Gym step. So one
``step`` always corresponds to one of the agent's own factored sub-steps, and the terminal reward
(+1 win / −1 loss / 0 draw) is unambiguously from ``agent_seat``'s perspective — which makes
"win-rate vs random" directly measurable (the M1 exit criterion).

Representation-agnostic seam: the observation is a ``gym.spaces.Dict`` whose shapes are read from
the Rust extension (``PyGame.obs_spec()``) and the action space is ``Discrete(PyGame.action_dim())``
— nothing is hard-coded here, so the Rust encoder/codec can be revised without touching this env.
The factored action mask for the current sub-step is always in ``info["action_mask"]`` (and via
``action_masks()`` for sb3-contrib's ``ActionMasker``).
"""

from __future__ import annotations

import numpy as np

try:
    import gymnasium as gym
    from gymnasium import spaces

    _GymEnv = gym.Env
except Exception:  # pragma: no cover
    gym = None
    spaces = None
    _GymEnv = object

import mtg_py

_U64 = (1 << 64) - 1
_INT_HIGH = (1 << 31) - 1


class MtgEnv(_GymEnv):
    metadata = {"render_modes": []}

    def __init__(self, deck="demo", auto_pass=True, agent_seat=0, opponent="random",
                 max_decisions=200_000):
        super().__init__()
        if spaces is None:
            raise ImportError("gymnasium is required for MtgEnv")
        self.deck = deck
        self.auto_pass = auto_pass
        self.agent_seat = int(agent_seat)
        self.opponent = opponent
        self.max_decisions = int(max_decisions)

        self._game = mtg_py.PyGame(deck, auto_pass)
        self.action_dim = mtg_py.PyGame.action_dim()
        self._spec = mtg_py.PyGame.obs_spec()  # [(name, rows, cols, is_int)]

        self.observation_space = spaces.Dict(
            {name: self._box(rows, cols, is_int) for (name, rows, cols, is_int) in self._spec}
        )
        self.action_space = spaces.Discrete(self.action_dim)

        self._decisions = 0
        self._terminal = True
        self._mask = np.zeros(self.action_dim, dtype=bool)
        self._opp_rng = np.random.default_rng(0)

    # ── Gym API ─────────────────────────────────────────────────────────────────────────────
    def reset(self, *, seed=None, options=None):
        super().reset(seed=seed)
        if seed is None:
            seed = int(self.np_random.integers(0, _U64, dtype=np.uint64))
        self._opp_rng = np.random.default_rng((int(seed) ^ 0x5DEECE66D) & _U64)
        self._decisions = 0
        step = self._game.reset(int(seed) & _U64)
        return self._advance_to_agent(step)

    def step(self, action):
        if self._terminal:
            raise RuntimeError("step() on a terminated episode; call reset() first")
        self._game.apply(int(action))
        self._decisions += 1
        step = self._game.step_to_decision()
        obs, info = self._advance_to_agent(step)
        terminated = self._terminal
        truncated = (not terminated) and self._decisions >= self.max_decisions
        reward = self._terminal_reward() if terminated else 0.0
        return obs, reward, terminated, truncated, info

    # ── opponent skipping + obs assembly ────────────────────────────────────────────────────
    def _advance_to_agent(self, step):
        """Answer every non-agent decision internally until it is the agent's turn (or terminal)."""
        obs_dict, mask, seat, request, num_legal, terminal = step
        while (not terminal) and seat != self.agent_seat:
            a = self._opponent_action(mask)
            self._game.apply(int(a))
            self._decisions += 1
            obs_dict, mask, seat, request, num_legal, terminal = self._game.step_to_decision()
        self._terminal = bool(terminal)
        self._mask = np.asarray(mask, dtype=bool)
        info = {
            "action_mask": self._mask,
            "seat": int(seat),
            "request": request,
            "num_legal": int(num_legal),
        }
        if self._terminal:
            info["summary"] = self.summary()
        return self._to_obs(obs_dict), info

    def _opponent_action(self, mask):
        legal = np.flatnonzero(np.asarray(mask, dtype=bool))
        assert legal.size >= 1, "empty mask for opponent decision"
        if self.opponent == "random":
            return int(self._opp_rng.choice(legal))
        # A callable opponent(obs_dict, mask) -> action index (for frozen-checkpoint self-play, M2).
        raise ValueError(f"unknown opponent {self.opponent!r}")

    def _to_obs(self, obs_dict):
        out = {}
        for (name, rows, cols, is_int) in self._spec:
            dtype = np.int64 if is_int else np.float32
            arr = np.asarray(obs_dict[name], dtype=dtype)
            out[name] = arr.reshape((cols,) if rows == 1 else (rows, cols))
        return out

    def _terminal_reward(self):
        winner = self._game.outcome()
        if winner is None:
            return 0.0
        return 1.0 if winner == self.agent_seat else -1.0

    # ── sb3-contrib MaskablePPO hooks ───────────────────────────────────────────────────────
    def action_masks(self):
        return self._mask.copy()

    def action_mask(self):  # alias
        return self._mask.copy()

    @staticmethod
    def _box(rows, cols, is_int):
        shape = (cols,) if rows == 1 else (rows, cols)
        if is_int:
            return spaces.Box(low=0, high=_INT_HIGH, shape=shape, dtype=np.int64)
        return spaces.Box(low=-np.inf, high=np.inf, shape=shape, dtype=np.float32)

    def summary(self):
        s = self._game.summary()
        if s is None:
            return None
        winner, turns, reason, init_objs, objs, zone_sum = s
        return {
            "winner": winner,
            "turns": turns,
            "reason": reason,
            "initial_object_count": init_objs,
            "object_count": objs,
            "zone_sum": zone_sum,
        }


def random_action(mask, rng) -> int:
    """Uniformly pick a legal action index from a boolean ``mask`` (trivial policy / opponent)."""
    legal = np.flatnonzero(np.asarray(mask, dtype=bool))
    if legal.size == 0:
        raise AssertionError("empty action mask — engine surfaced a decision with no legal option")
    return int(rng.choice(legal))
