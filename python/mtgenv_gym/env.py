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
                 max_decisions=3000, record_replay=False, replay_step=0):
        super().__init__()
        if spaces is None:
            raise ImportError("gymnasium is required for MtgEnv")
        self.deck = deck
        self.auto_pass = auto_pass
        self.agent_seat = int(agent_seat)
        self.opponent = opponent
        self.max_decisions = int(max_decisions)
        self.record_replay = bool(record_replay)
        self.replay_step = int(replay_step)

        self._game = mtg_py.PyGame(deck, auto_pass, self.record_replay, self.replay_step)
        self.action_dim = mtg_py.PyGame.action_dim()
        self._spec = mtg_py.PyGame.obs_spec()  # [(name, rows, cols, is_int)]

        # Deck-determined card-identity ONE-HOT (GYM_PLAN §3). The Rust encoder stays card-agnostic
        # (it emits a per-row `grp_id` in `*_ids`); here — the swappable Python seam — we map that to
        # an explicit one-hot whose categories are fixed up front from the matchup's unique cards
        # (`PyGame.card_vocab()` = union of both decks' grp_ids) + a token reserve + an "unknown"
        # catch-all. Collision-free and interpretable, unlike the hashed `grp_id` embedding (which we
        # also keep). One-hot per card-bearing row of every entity table (battlefield/hand/stack).
        TOKEN_RESERVE = 8
        vocab = [int(g) for g in self._game.card_vocab()]
        self._cardid_index = {g: i for i, g in enumerate(vocab)}
        self._cardid_dim = len(vocab) + TOKEN_RESERVE + 1  # + reserved token slots + unknown (last)
        self._cardid_unknown = self._cardid_dim - 1
        # Row count of each card-bearing table, read from the encoder spec (`*_ids` is (1, R)).
        self._cardid_tables = {
            name[:-4]: cols for (name, rows, cols, is_int) in self._spec if name.endswith("_ids")
        }

        obs_spaces = {name: self._box(rows, cols, is_int) for (name, rows, cols, is_int) in self._spec}
        for tbl, rows in self._cardid_tables.items():
            obs_spaces[f"{tbl}_cardid"] = spaces.Box(
                low=0.0, high=1.0, shape=(rows, self._cardid_dim), dtype=np.float32
            )
        self.observation_space = spaces.Dict(obs_spaces)
        self.action_space = spaces.Discrete(self.action_dim)

        self._decisions = 0
        self._terminal = True
        self._mask = np.zeros(self.action_dim, dtype=bool)
        self._opp_rng = np.random.default_rng(0)
        # External-opponent state machine (see the ext_* methods); inert in the standard path.
        self._where = "terminal"
        self._truncated = False
        self._cur_obs = None
        self._last_learner_obs = None

    # ── Gym API ─────────────────────────────────────────────────────────────────────────────
    def reset(self, *, seed=None, options=None):
        super().reset(seed=seed)
        if seed is None:
            seed = int(self.np_random.integers(0, _U64, dtype=np.uint64))
        self._opp_rng = np.random.default_rng((int(seed) ^ 0x5DEECE66D) & _U64)
        # Resample the opponent for this episode (e.g. a fresh draw from the self-play pool).
        if hasattr(self.opponent, "reset"):
            self.opponent.reset(self._opp_rng)
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

    # ── external-opponent driving (BatchedSelfPlayVecEnv, GYM_PLAN §6) ────────────────────────
    # In this mode the env does NOT resolve opponent decisions itself. It advances to the next
    # *pause* — a learner decision, an opponent decision, or terminal — and the caller (the batched
    # self-play pump) supplies the action for whichever seat is to move. Surfacing opponent
    # decisions instead of answering them inline is what lets opponent inference be batched across
    # many games into one forward (the per-env synchronous predict is the training bottleneck).
    # Each pause is one factored sub-step (the same Discrete(action_dim) the learner uses), so the
    # opponent policy is queried exactly like the learner. The same "advance to a decision, let an
    # external evaluator answer a batch" shape is what tree search (MCTS leaf eval) will reuse.
    def ext_reset(self, seed):
        """Start a fresh game and advance to the first pause. Inspect with ``ext_state()``."""
        self._opp_rng = np.random.default_rng((int(seed) ^ 0x5DEECE66D) & _U64)
        self._decisions = 0
        self._truncated = False
        self._last_learner_obs = None
        self._ext_advance(self._game.reset(int(seed) & _U64))

    def _ext_advance(self, step):
        obs_dict, mask, seat, request, num_legal, terminal = step
        self._cur_obs = obs_dict
        self._mask = np.asarray(mask, dtype=bool)
        capped = self._decisions >= self.max_decisions
        if terminal or capped:
            self._terminal = bool(terminal)
            self._truncated = capped and not terminal
            self._where = "terminal"
            return
        self._where = "learner" if seat == self.agent_seat else "opponent"
        if self._where == "learner":
            self._last_learner_obs = obs_dict

    def ext_state(self):
        """``'learner'`` | ``'opponent'`` | ``'terminal'`` — who (if anyone) must act now."""
        return self._where

    def ext_obs(self):
        """Encoded observation for the seat currently to move (learner or opponent)."""
        return self._to_obs(self._cur_obs)

    def ext_mask(self):
        """Legal-action mask for the seat currently to move."""
        return self._mask.copy()

    def ext_apply(self, action):
        """Apply one factored sub-step for the current actor and advance to the next pause."""
        self._game.apply(int(action))
        self._decisions += 1
        self._ext_advance(self._game.step_to_decision())

    def ext_reward(self):
        """Terminal reward from the learner's (``agent_seat``) perspective (+1/0/−1)."""
        return self._terminal_reward()

    def ext_truncated(self):
        return self._truncated

    def ext_last_learner_obs(self):
        """The last obs the learner actually saw (a valid-shaped ``terminal_observation``; unused
        for value bootstrapping on genuine terminations, where done masks it out)."""
        o = self._last_learner_obs if self._last_learner_obs is not None else self._cur_obs
        return self._to_obs(o)

    # ── opponent skipping + obs assembly ────────────────────────────────────────────────────
    def _advance_to_agent(self, step):
        """Answer every non-agent decision internally until it is the agent's turn (or terminal)."""
        obs_dict, mask, seat, request, num_legal, terminal = step
        while (not terminal) and seat != self.agent_seat:
            a = self._opponent_action(obs_dict, mask)
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

    def _opponent_action(self, obs_dict, mask):
        """Answer one opponent-seat decision. ``opponent`` is either ``"random"`` or a policy —
        a callable ``act(obs, mask) -> int`` or an object with such an ``.act`` (e.g. a frozen
        checkpoint / self-play pool member). The obs handed to a policy opponent is from the
        opponent seat's own perspective (the engine builds the view for the deciding seat), so a
        single relative-encoded policy plays both seats correctly."""
        legal = np.flatnonzero(np.asarray(mask, dtype=bool))
        assert legal.size >= 1, "empty mask for opponent decision"
        if self.opponent == "random":
            return int(self._opp_rng.choice(legal))
        act = self.opponent.act if hasattr(self.opponent, "act") else self.opponent
        return int(act(self._to_obs(obs_dict), np.asarray(mask, dtype=bool)))

    def _to_obs(self, obs_dict):
        out = {}
        for (name, rows, cols, is_int) in self._spec:
            dtype = np.int64 if is_int else np.float32
            arr = np.asarray(obs_dict[name], dtype=dtype)
            out[name] = arr.reshape((cols,) if rows == 1 else (rows, cols))
        # Card-identity one-hot per table, from each row's grp_id (`*_ids`; 0 = empty/hidden row →
        # all-zero). Unmapped present ids (e.g. a future token) fall in the reserved "unknown" slot.
        for tbl in self._cardid_tables:
            ids = out[f"{tbl}_ids"]
            oh = np.zeros((ids.shape[0], self._cardid_dim), dtype=np.float32)
            for r in np.flatnonzero(ids != 0):  # only present rows (few) — cheap
                oh[r, self._cardid_index.get(int(ids[r]), self._cardid_unknown)] = 1.0
            out[f"{tbl}_cardid"] = oh
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

    def export_replay(self, out_dir, created_at_ms, names=None, decks=None, run_name=None):
        """Write the just-finished game's omniscient replay JSON to ``out_dir`` (training-replay
        export, REPLAY_PLAN §3). Requires ``record_replay=True`` and a terminated episode. The
        filename embeds ``run_name`` (e.g. the TensorBoard run ``MaskablePPO_2``) so replays from
        different runs stay distinguishable in the lobby. Returns the written path, or ``None`` if
        no replay was recorded."""
        js = self._game.replay_json(int(created_at_ms), names, decks)
        if js is None:
            return None
        import os

        os.makedirs(out_dir, exist_ok=True)
        run = f"{run_name}-" if run_name else ""
        path = os.path.join(
            out_dir, f"aitrain-{run}{self.deck}-step{self.replay_step:07d}-{created_at_ms}.json"
        )
        with open(path, "w") as f:
            f.write(js)
        return path

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
