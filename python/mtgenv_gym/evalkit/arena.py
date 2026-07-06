"""``Arena`` — play N seeded games of one policy vs another and return a structured ``EvalResult``.

Algorithm-agnostic: both seats are driven by the :class:`~mtgenv_gym.evalkit.policy.Policy` protocol,
so *any* algorithm's adapter is evaluated the same way. The engine is a **batched lockstep pump** over
N ``MtgEnv(opponent="external")`` games (a generalization of ``BatchedSelfPlayVecEnv._pump`` to two
arbitrary policies with no training): every game is advanced to a decision, then all games waiting on
policy A are answered in one ``A.act`` call and all waiting on policy B in one ``B.act`` call. A plain
NN policy thus pays one batched forward per round instead of N single ones; a search policy runs its
per-root budget over the batch — the same structure batched-MCTS leaf eval will use.

Deterministic given ``(seed, n_games, batch_size)``: game *i* runs with engine seed ``seed + i`` and,
with a :class:`RandomPolicy` opponent, is **bit-identical** to the legacy
``MtgEnv(opponent="random")`` path (that policy replays the engine's own opponent RNG stream). The
evaluated policy is seat 0 (``agent_seat``); ``EvalResult`` is from its perspective.

Both eval modes are always available (``evaluate`` runs greedy AND sample — the MuZero lesson: greedy
is the headline, sampled is the honest learning-signal curve). The opponent runs in its own fixed mode
(default ``"sample"``, matching the legacy ``ModelOpponent(deterministic=False)`` self-play opponent).
"""

from __future__ import annotations

from dataclasses import asdict, dataclass, field

import numpy as np

from ..env import MtgEnv
from .analyzers import get_analyzer
from .metrics import OutcomeAgg, StatsAgg, wilson_ci

_U64 = (1 << 64) - 1


@dataclass
class EvalResult:
    """One (policy, opponent, mode) evaluation over ``n_games``. JSON-serializable via ``to_json``."""

    deck: str
    opponent: str
    mode: str                              # evaluated policy's mode: "greedy" | "sample"
    n_games: int
    wins: int
    losses: int
    draws: int
    win_rate: float
    win_ci95: "tuple[float, float]"
    avg_turns: float
    stats: "dict[str, float]"              # attack_rate, productive_rate, block_rate, …
    end_reasons: "dict[str, float]"        # {reason: fraction}
    seed: int
    analyzers: "dict[str, float]" = field(default_factory=dict)

    def to_json(self) -> dict:
        d = asdict(self)
        d["win_ci95"] = list(self.win_ci95)
        return d

    def __str__(self) -> str:
        lo, hi = self.win_ci95
        s = (f"{self.deck} vs {self.opponent} [{self.mode}] n={self.n_games}: "
             f"win={self.win_rate:.3f} (95% {lo:.2f}-{hi:.2f}) "
             f"turns={self.avg_turns:.1f}")
        if self.stats:
            s += "  " + " ".join(f"{k}={v:.2f}" for k, v in self.stats.items())
        return s


class Arena:
    """Reusable batched game runner for one deck. Create once, call ``play``/``evaluate`` many times."""

    def __init__(self, deck: str, *, batch_size: "int | None" = None, max_decisions: int = 3000,
                 agent_seat: int = 0, analyzers: bool = True):
        self.deck = deck
        self.max_decisions = int(max_decisions)
        self.agent_seat = int(agent_seat)
        self.batch_size = batch_size
        self._use_analyzers = analyzers
        self._envs: "list[MtgEnv]" = []

    def _ensure_envs(self, k: int) -> None:
        while len(self._envs) < k:
            self._envs.append(MtgEnv(deck=self.deck, opponent="external",
                                     max_decisions=self.max_decisions))

    @staticmethod
    def _seed_global_rng(seed: int) -> None:
        """Seed numpy + torch (if present) so a sampled SB3 opponent / policy is reproducible."""
        np.random.seed(seed & 0x7FFF_FFFF)
        try:  # torch only when a torch policy is actually in play
            import torch

            torch.manual_seed(seed & _U64)
        except Exception:
            pass

    def play(self, policy_a, policy_b, *, n_games: int, seed: int = 0, a_mode: str = "greedy",
             b_mode: str = "sample", opponent_label: str = "opponent") -> EvalResult:
        """Evaluate ``policy_a`` (seat ``agent_seat``, mode ``a_mode``) vs ``policy_b`` (mode
        ``b_mode``) over ``n_games`` seeded games. Deterministic given ``(seed, n_games, batch_size)``."""
        self._seed_global_rng(seed)
        B = self.batch_size or min(n_games, 64)
        self._ensure_envs(min(B, n_games))
        outcomes = OutcomeAgg()
        stats = StatsAgg()
        analyzer = get_analyzer(self.deck) if self._use_analyzers else None

        for w0 in range(0, n_games, B):
            gidx = list(range(w0, min(w0 + B, n_games)))
            self._run_wave(gidx, seed, policy_a, policy_b, a_mode, b_mode, outcomes, stats, analyzer)

        wins, n = outcomes.wins, outcomes.n
        return EvalResult(
            deck=self.deck, opponent=opponent_label, mode=a_mode, n_games=n,
            wins=outcomes.wins, losses=outcomes.losses, draws=outcomes.draws,
            win_rate=outcomes.win_rate(), win_ci95=wilson_ci(wins, n),
            avg_turns=outcomes.avg_turns(), stats=stats.as_dict(),
            end_reasons=outcomes.end_reason_fracs(), seed=int(seed),
            analyzers=(analyzer.result() if analyzer is not None else {}),
        )

    def _run_wave(self, gidx, seed, policy_a, policy_b, a_mode, b_mode, outcomes, stats, analyzer):
        envs = self._envs
        slots = list(range(len(gidx)))                 # local env-object slots for this wave
        game_of = {s: gidx[s] for s in slots}          # slot -> global game index
        game_seeds = {s: seed + gidx[s] for s in slots}

        for s in slots:
            envs[s].ext_reset(game_seeds[s] & _U64)
        self._reset_policy(policy_a, [game_of[s] for s in slots], seed)
        self._reset_policy(policy_b, [game_of[s] for s in slots], seed)

        active = set(slots)
        for _ in range(200_000):  # guard: a pathological non-terminating batch
            a_pending, b_pending = [], []
            for s in list(active):
                st = envs[s].ext_state()
                if st == "terminal":
                    outcomes.add(envs[s].ext_reward(), envs[s].summary())
                    active.discard(s)
                elif st == "learner":
                    a_pending.append(s)
                else:
                    b_pending.append(s)
            if not active:
                return
            if a_pending:
                obs = [envs[s].ext_obs() for s in a_pending]
                masks = [envs[s].ext_mask() for s in a_pending]
                acts = policy_a.act(obs, masks, mode=a_mode,
                                    env_indices=[game_of[s] for s in a_pending])
                for k, s in enumerate(a_pending):
                    envs[s].ext_apply(int(acts[k]))
                    rec = envs[s].ext_take_stats()  # evaluated-policy (seat-0) decision stats
                    if rec:
                        stats.update(rec)
                        if analyzer is not None:
                            analyzer.observe(obs[k], rec)
            if b_pending:
                obs = [envs[s].ext_obs() for s in b_pending]
                masks = [envs[s].ext_mask() for s in b_pending]
                acts = policy_b.act(obs, masks, mode=b_mode,
                                    env_indices=[game_of[s] for s in b_pending])
                for k, s in enumerate(b_pending):
                    envs[s].ext_apply(int(acts[k]))
                    envs[s].ext_take_stats()  # drain (discard) — never attribute opponent stats to A
        raise RuntimeError("Arena wave did not converge to terminal — check for a non-terminating game")

    @staticmethod
    def _reset_policy(policy, game_indices, seed):
        reset = getattr(policy, "reset", None)
        if reset is None:
            return
        try:
            reset(game_indices, rng=np.random.default_rng(seed & _U64),
                  game_seeds=[seed + g for g in game_indices])
        except TypeError:  # a minimal policy whose reset takes only (env_indices)
            reset(game_indices)

    # ── the both-modes convenience (MuZero lesson) ─────────────────────────────────────────────────
    def evaluate(self, policy_a, policy_b, *, n_games: int, seed: int = 0, b_mode: str = "sample",
                 opponent_label: str = "opponent",
                 modes=("greedy", "sample")) -> "dict[str, EvalResult]":
        """Run ``play`` in each of ``modes`` (greedy AND sample by default). Returns ``{mode: result}``."""
        return {m: self.play(policy_a, policy_b, n_games=n_games, seed=seed, a_mode=m, b_mode=b_mode,
                             opponent_label=opponent_label) for m in modes}
