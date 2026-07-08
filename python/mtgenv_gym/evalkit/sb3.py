"""SB3 / MaskablePPO integration for evalkit.

Two things live here (both isolated so importing the rest of evalkit stays torch/sb3-free):

* :class:`SB3Policy` — the adapter that makes a MaskablePPO checkpoint (or live model) a ``Policy``.
  Batched via ``mtgenv_gym.inference.BatchedPolicy`` (one forward per Arena round).
* :class:`EvalkitCallback` — the drop-in training hook that replaces ``SelfPlayEval`` +
  ``LadderEval`` + ``ReplayCheckpoint`` in ``selfplay_train.py`` with **tag-set-identical** defaults:
  ``selfplay/winrate_vs_random``, ``selfplay/winrate_vs_initial``, ``ladder/winrate_vs_NNpct`` at the
  same seed bases (5e6 / 6e6 / 7e6+pct) and milestones (10/25/50/75%), plus the intended additions
  (the ``_sampled`` variants — MuZero lesson — and deck analyzers when the deck has one). Behaviour
  stats (``stats/*``) and game length (``game/*``) remain owned by ``TrackedStatsCallback`` /
  ``GameLengthCallback`` on the training rollout stream — evalkit does not double-log them here.
"""

from __future__ import annotations

import os

from stable_baselines3.common.callbacks import BaseCallback

from ..inference import BatchedPolicy
from .arena import Arena
from .ladder import Ladder
from .policy import BasePolicy, RandomPolicy
from .replay import REPLAY_DIR, record_game
from .scripted import ScriptedPolicy
from .tb_logging import SB3Recorder, eval_seed, log_eval, write_json


def load_elo_opponents(env, top=3, device="cpu", exclude=()):
    """Top-``top`` rated agents from ``data/elo/<env>/ratings.json`` as ``[(name, Policy), …]`` — READ
    ONLY, fail-soft. Reuses rate_agent's canonical loader (handles kinds + older obs widths via its
    schema adapter). Excludes the random anchor + any name in ``exclude`` (e.g. the current run / self).
    Returns ``[]`` with no ratings file or on any error, so a run without a rating pool just skips it."""
    try:
        import json

        from .ratings import ANCHOR_NAME, elo_root, load_registry

        rpath = elo_root() / env / "ratings.json"
        if not rpath.is_file():
            return []
        ratings = json.loads(rpath.read_text())["agents"]
        reg = load_registry(env)
        import rate_agent  # canonical build_policy (+ obs-schema adapter); read-only use of its code

        skip = {ANCHOR_NAME, "random"} | set(x for x in exclude if x)
        out = []
        for name, r in sorted(ratings.items(), key=lambda kv: -kv[1].get("rating", 0)):
            if name in skip or name not in reg:
                continue
            try:
                out.append((name, rate_agent.build_policy(reg[name], device=device)))
            except Exception:
                continue  # unloadable (bad ckpt / kind) → skip, keep scanning
            if len(out) >= top:
                break
        return out
    except Exception:
        return []


class SB3Policy(BasePolicy):
    """A MaskablePPO model (or checkpoint path) as an evalkit ``Policy``. Wraps a live model too — its
    ``BatchedPolicy`` reads the model's policy object, so weights are always current."""

    def __init__(self, model_or_path, device: str = "cpu"):
        if isinstance(model_or_path, str):
            from sb3_contrib import MaskablePPO
            from mtgenv_gym.policy import EntityExtractor  # noqa: F401 — needed to unpickle extractor

            model_or_path = MaskablePPO.load(model_or_path, device=device)
        self.model = model_or_path
        self._bp = BatchedPolicy(self.model)

    @classmethod
    def load(cls, path: str, device: str = "cpu") -> "SB3Policy":
        return cls(path, device=device)

    def act(self, obs_batch, mask_batch, *, mode="greedy", env_indices=None):
        return self._bp.act(obs_batch, mask_batch, deterministic=(mode == "greedy"))


class EvalkitCallback(BaseCallback):
    """Drop-in eval hook: win-rate vs random + vs initial, the %-trained ladder, and periodic replays.

    Tag-set-identical to the legacy trio by default (see module docstring). ``TrackedStats`` /
    ``GameLength`` are left in place by ``selfplay_train`` — this owns only the eval-derived curves."""

    def __init__(self, deck, *, total_timesteps, eval_freq, n_envs, ref_path,
                 ladder_dir, n_games=40, milestones=(0.10, 0.25, 0.50, 0.75),
                 replay_every=0, replay_dir=REPLAY_DIR, run_name=None, device="cpu",
                 batch_size=64, seed_random=5_000_000, seed_initial=6_000_000,
                 seed_script=8_000_000, eval_script=True, eval_elo=True, elo_top=3,
                 seed_elo=9_500_000, json_dir=None, log_sampled=True, verbose=0):
        super().__init__(verbose)
        self.deck = deck
        self.total = max(int(total_timesteps), 1)
        self.every = max(eval_freq // n_envs, 1)
        self.ref_path = ref_path
        self.ladder_dir = ladder_dir
        self.n_games = n_games
        self.milestones = milestones
        self.replay_every = max(replay_every // n_envs, 1) if replay_every > 0 else 0
        self.replay_dir = replay_dir
        self.run_name = run_name
        self.device = device
        self.batch_size = batch_size
        self.seed_random = seed_random
        self.seed_initial = seed_initial
        self.seed_script = seed_script
        self.eval_script = eval_script
        self.json_dir = json_dir
        self.log_sampled = log_sampled
        self._modes = ("greedy", "sample") if log_sampled else ("greedy",)
        self._arena = None
        self._policy = None
        self.eval_elo = eval_elo
        self.elo_top = int(elo_top)
        self.seed_elo = seed_elo
        self._rec = None
        self._ladder = None
        self._ref = None
        self._script = None
        self._elo = []  # [(name, Policy), ...] top-rated agents from data/elo/<deck>/ratings.json

    # ── lifecycle ──────────────────────────────────────────────────────────────────────────────
    def _on_training_start(self) -> None:
        self._arena = Arena(self.deck, batch_size=self.batch_size)
        self._policy = SB3Policy(self.model, device=self.device)
        self._script = ScriptedPolicy() if self.eval_script else None
        if self.eval_elo:
            self._elo = load_elo_opponents(self.deck, self.elo_top, self.device,
                                           exclude=(self.run_name,))
            if self._elo:
                names = ", ".join(f"elo{i+1}={n}" for i, (n, _) in enumerate(self._elo))
                from ..tb_meta import _writer

                w = _writer(self.model)
                if w is not None:
                    w.add_text("run/elo_opponents", names, 0)  # so a reader knows which agent each rank is
                if self.verbose:
                    print(f"  elo opponents: {names}")
        self._rec = SB3Recorder(self.logger)
        self._ladder = Ladder(self.ladder_dir, self._snapshot_fn, self._load_snapshot,
                              milestones=self.milestones, n_games=self.n_games)
        if self.run_name is None:
            from ..replays import run_name_of

            self.run_name = run_name_of(self.model)
        if self.replay_every:  # step-0 replay of the random-init policy (matches ReplayCheckpoint)
            record_game(self._policy, self.deck, 0, self_play=True, out_dir=self.replay_dir,
                        run_name=self.run_name, algo="PPO")

    def _snapshot_fn(self, path):
        self.model.save(path)   # writes <path>.zip
        return path + ".zip"

    def _load_snapshot(self, path):
        return SB3Policy(path, device=self.device)

    # ── the periodic eval ──────────────────────────────────────────────────────────────────────
    def _on_step(self) -> bool:
        if self.replay_every and self.n_calls % self.replay_every == 0:
            record_game(self._policy, self.deck, self.num_timesteps, self_play=True,
                        out_dir=self.replay_dir, run_name=self.run_name, algo="PPO")
        if self.n_calls % self.every == 0:
            self._eval()
        return True

    def _eval(self) -> None:
        step = self.num_timesteps
        labelled = {}
        # Rotating per-eval seeds (tb_logging.eval_seed): each opponent's games shift by `step` so
        # consecutive evals sample fresh games (no frozen test set). Rotating the Arena seed also
        # rotates the opponent's per-game rng (Arena reseeds it from seed+g); we bump the opponent's
        # constructor seed too for good measure.
        s_rand = eval_seed(self.seed_random, step)
        s_init = eval_seed(self.seed_initial, step)
        s_scr = eval_seed(self.seed_script, step)

        # vs random — winrate (+sampled) + analyzers (eval-derived); stats/game stay with TrackedStats.
        vr = self._arena.evaluate(self._policy, RandomPolicy(seed=s_rand),
                                  n_games=self.n_games, seed=s_rand,
                                  opponent_label="random", modes=self._modes)
        log_eval(self._rec, vr, win_tag="selfplay/winrate_vs_random", step=step,
                 with_stats=False, with_game=False, with_analyzers=True)
        for m, r in vr.items():
            labelled[f"random_{m}"] = r

        # vs initial (random-init reference) — winrate only, NaN until the ref exists (legacy).
        if self._ref is None and os.path.exists(self.ref_path):
            self._ref = SB3Policy(self.ref_path, device=self.device)
        if self._ref is not None:
            vi = self._arena.evaluate(self._policy, self._ref, n_games=self.n_games,
                                      seed=s_init, opponent_label="initial",
                                      modes=self._modes)
            log_eval(self._rec, vi, win_tag="selfplay/winrate_vs_initial", step=step,
                     with_stats=False, with_game=False, with_analyzers=False)
            for m, r in vi.items():
                labelled[f"initial_{m}"] = r
        else:
            self._rec.record("selfplay/winrate_vs_initial", float("nan"), step)

        # vs the scripted reference (land>spell>attack-all, never block) — the standing yardstick.
        # ≈0.5 means the agent has learned the deck. Winrate (+sampled) only; a fixed opponent so no
        # stats/analyzers (those stay with the vs-random source). Same canonical schema as the others.
        if self._script is not None:
            vs = self._arena.evaluate(self._policy, self._script, n_games=self.n_games,
                                      seed=s_scr, opponent_label="script", modes=self._modes)
            log_eval(self._rec, vs, win_tag="selfplay/winrate_vs_script", step=step,
                     with_stats=False, with_game=False, with_analyzers=False)
            for m, r in vs.items():
                labelled[f"script_{m}"] = r

        # vs the top-N Elo-rated agents (data/elo/<deck>) — winrate (+sampled) per rank. The user's
        # idea: measure progress against the actual strongest agents, not just random/script. Distinct
        # rotating seed per rank; fixed opponents so no stats/analyzers.
        for rank, (name, opp) in enumerate(self._elo, 1):
            ve = self._arena.evaluate(self._policy, opp, n_games=self.n_games,
                                      seed=eval_seed(self.seed_elo + rank * 100_000, step),
                                      opponent_label=f"elo{rank}:{name}", modes=self._modes)
            log_eval(self._rec, ve, win_tag=f"selfplay/winrate_vs_elo{rank}", step=step,
                     with_stats=False, with_game=False, with_analyzers=False)
            for m, r in ve.items():
                labelled[f"elo{rank}_{m}"] = r

        # %-trained ladder (framework-managed snapshots).
        self._ladder.maybe_snapshot(self.num_timesteps / self.total)
        self._ladder.eval_and_log(self._policy, self._arena, self._rec, step=step)

        if self.json_dir and labelled:
            write_json(self.json_dir, step, labelled)
        if self.verbose:
            g = vr["greedy"]
            print(f"  [{step}] vs_random={g.win_rate:.2f} (95% {g.win_ci95[0]:.2f}-{g.win_ci95[1]:.2f})")
