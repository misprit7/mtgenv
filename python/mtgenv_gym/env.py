"""``MtgEnv`` — the milestone-0 Gymnasium wrapper over ``mtg_py.PyGame`` (GYM_PLAN L4).

The heavy lifting lives in Rust: the engine runs on its own thread, the observation encoder and
action codec are Rust modules behind a stable FFI seam (``mtg_py``). This file is deliberately
thin and **representation-agnostic** — the observation width and action vocabulary are read from
the extension (``PyGame.obs_dim()`` / ``PyGame.action_dim()``), never hard-coded — so milestone 1
can swap the encoder/codec on the Rust side without touching this env or the training loop.

Self-play shape: the env holds **both seats**. Every decision (whichever seat must act) surfaces
through the same ``step`` interface with its own legal-action mask in ``info["action_mask"]``; the
caller's policy answers it. Terminal reward is from ``agent_seat``'s perspective (+1 win / -1 loss
/ 0 draw). A real opponent-routing / two-policy split is milestone 1+; for the random self-play
smoke a single random policy answers both seats.
"""

from __future__ import annotations

import numpy as np

try:
    import gymnasium as gym
    from gymnasium import spaces

    _GymEnv = gym.Env
except Exception:  # pragma: no cover - gym is required for the env but not the low-level driver
    gym = None
    spaces = None
    _GymEnv = object

import mtg_py

_U64 = (1 << 64) - 1


class MtgEnv(_GymEnv):
    """A single mtg-core game as a Gymnasium environment.

    Parameters
    ----------
    deck: one of ``"lands"``, ``"demo"``, ``"burn_vs_bears"``.
    auto_pass: enable the engine's Arena-profile auto-pass (fewer trivial priority windows).
    agent_seat: which seat the terminal reward is scored for (0 or 1).
    max_decisions: truncation cap on decisions per episode (mirror-stall backstop).
    """

    metadata = {"render_modes": []}

    def __init__(self, deck="demo", auto_pass=True, agent_seat=0, max_decisions=200_000):
        super().__init__()
        if spaces is None:
            raise ImportError("gymnasium is required for MtgEnv; install it or use the low-level driver")
        self.deck = deck
        self.auto_pass = auto_pass
        self.agent_seat = int(agent_seat)
        self.max_decisions = int(max_decisions)

        self._game = mtg_py.PyGame(deck, auto_pass)
        self.obs_dim = mtg_py.PyGame.obs_dim()
        self.action_dim = mtg_py.PyGame.action_dim()
        self.observation_space = spaces.Box(
            low=-np.inf, high=np.inf, shape=(self.obs_dim,), dtype=np.float32
        )
        self.action_space = spaces.Discrete(self.action_dim)

        self._decisions = 0
        self._terminal = True
        self._pending_mask = None

    # ── Gym API ─────────────────────────────────────────────────────────────────────────────
    def reset(self, *, seed=None, options=None):
        super().reset(seed=seed)
        if seed is None:
            seed = int(self.np_random.integers(0, _U64, dtype=np.uint64))
        self._decisions = 0
        step = self._game.reset(int(seed) & _U64)
        return self._unpack(step)

    def step(self, action):
        if self._terminal:
            raise RuntimeError("step() called on a terminated episode; call reset() first")
        self._game.apply(int(action))
        self._decisions += 1
        step = self._game.step_to_decision()
        obs, info = self._unpack(step)
        terminated = self._terminal
        truncated = (not terminated) and self._decisions >= self.max_decisions
        reward = self._terminal_reward() if terminated else 0.0
        return obs, reward, terminated, truncated, info

    # ── helpers ─────────────────────────────────────────────────────────────────────────────
    def _unpack(self, step):
        obs, mask, seat, request, num_legal, terminal = step
        self._terminal = bool(terminal)
        mask = np.asarray(mask, dtype=bool)
        self._pending_mask = mask
        info = {
            "action_mask": mask,
            "seat": int(seat),
            "request": request,
            "num_legal": int(num_legal),
        }
        if self._terminal:
            info["summary"] = self.summary()
        return np.asarray(obs, dtype=np.float32), info

    def _terminal_reward(self):
        winner = self._game.outcome()
        if winner is None:
            return 0.0
        return 1.0 if winner == self.agent_seat else -1.0

    def action_mask(self):
        """The current legal-action mask (bool array of width ``action_dim``)."""
        return np.asarray(self._game.legal_mask(), dtype=bool)

    def summary(self):
        """Terminal ``(winner, turns, reason, initial_objects, objects, zone_sum)`` or ``None``."""
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
    """Uniformly pick a legal action index from a boolean ``mask`` (the trivial M0 policy)."""
    legal = np.flatnonzero(np.asarray(mask, dtype=bool))
    if legal.size == 0:
        raise AssertionError("empty action mask — engine surfaced a decision with no legal option")
    return int(rng.choice(legal))
