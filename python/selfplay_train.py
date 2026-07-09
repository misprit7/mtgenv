"""Self-play training (GYM_PLAN §8.2): train a MaskablePPO policy against a growing pool of frozen
copies of itself (a self-play league), on the demo deck (mirror). Logs win-rate vs random and vs
the initial (random-init) checkpoint to TensorBoard — "stable self-play improvement" is the M2 exit
signal (the policy keeps beating its past selves while staying strong vs random).

    PYTHONPATH=python python python/selfplay_train.py --timesteps 120000 --tensorboard /home/xander/dev/p-mtg/mtgenv/data/tb
    tensorboard --logdir /home/xander/dev/p-mtg/mtgenv/data/tb

``train_selfplay`` is importable so the learning-sanity test can run a short version.
"""

from __future__ import annotations

import argparse
import glob
import os

from sb3_contrib import MaskablePPO
from sb3_contrib.common.wrappers import ActionMasker
from stable_baselines3.common.callbacks import BaseCallback
from stable_baselines3.common.vec_env import DummyVecEnv, SubprocVecEnv

from mtgenv_gym import MtgEnv
from mtgenv_gym.league import ModelOpponent, OpponentPool, PoolCheckpoint
from mtgenv_gym.attn_policy import RelationalPointerPolicy
from mtgenv_gym.policy import EntityExtractor

DEFAULT_POOL = "/tmp/mtgenv_pool"


def _mask_fn(env):
    return env.action_masks()


def make_env(deck, pool_dir, seed):
    """Factory (picklable for SubprocVecEnv) for one self-play env with its own opponent pool."""
    def thunk():
        env = MtgEnv(deck=deck, opponent=OpponentPool(pool_dir, rng_seed=seed))
        return ActionMasker(env, _mask_fn)

    return thunk


def make_vecenv(deck, pool_dir, n_envs, seed, subproc=False, batched=True, p_random=0.2, device=None,
                vecenv="batched", num_workers=8, p_script=0.0, script_mix=None):
    """Self-play vec env. ``vecenv``: ``"batched"`` (#41, single-threaded Python pump) or ``"fleet"``
    (M3.4, ``mtg_py.Fleet`` worker-thread parallel stepping — same self-play regime, stepping in Rust).
    ``batched=False`` is the legacy ``DummyVecEnv`` per-env path (``subproc`` applies only there).
    ``p_script``/``script_mix`` mix punisher heuristics into the opponent pool (fleet + batched only)."""
    if vecenv == "fleet":
        from mtgenv_gym import FleetSelfPlayVecEnv

        if device is None:
            import torch

            device = "cuda" if torch.cuda.is_available() else "cpu"
        return FleetSelfPlayVecEnv(deck, pool_dir, n_envs, num_workers=num_workers, p_random=p_random,
                                   seed=seed, device=device, p_script=p_script, script_mix=script_mix)
    if batched:
        from mtgenv_gym import BatchedSelfPlayVecEnv

        if device is None:
            import torch

            device = "cuda" if torch.cuda.is_available() else "cpu"
        return BatchedSelfPlayVecEnv(deck, pool_dir, n_envs, p_random=p_random, seed=seed,
                                     device=device, p_script=p_script, script_mix=script_mix)
    factories = [make_env(deck, pool_dir, seed * 100 + i) for i in range(n_envs)]
    if subproc:
        # spawn (not fork) so each worker re-imports torch cleanly — fork + torch is fragile.
        return SubprocVecEnv(factories, start_method="spawn")
    return DummyVecEnv(factories)


def play_winrate(model, deck, opponent, n_games, seed0):
    """Greedy win-rate of ``model`` (seat 0) vs ``opponent`` over ``n_games``."""
    env = MtgEnv(deck=deck, opponent=opponent)
    wins = 0
    for i in range(n_games):
        obs, info = env.reset(seed=seed0 + i)
        done = False
        reward = 0.0
        while not done:
            action, _ = model.predict(obs, action_masks=info["action_mask"], deterministic=True)
            obs, reward, term, trunc, info = env.step(int(action))
            done = term or trunc
        wins += 1 if reward > 0 else 0
    return wins / n_games


class SelfPlayEval(BaseCallback):
    """Periodically log win-rate vs random and vs the initial (random-init) checkpoint."""

    def __init__(self, deck, ref_path, eval_freq, n_envs, n_games=40, verbose=0):
        super().__init__(verbose)
        self.deck = deck
        self.ref_path = ref_path
        self.every = max(eval_freq // n_envs, 1)
        self.n_games = n_games
        self._ref = None

    def _on_step(self) -> bool:
        if self.n_calls % self.every == 0:
            wr_rand = play_winrate(self.model, self.deck, "random", self.n_games, 5_000_000)
            if self._ref is None and os.path.exists(self.ref_path):
                self._ref = ModelOpponent(self.ref_path)
            wr_init = (
                play_winrate(self.model, self.deck, self._ref, self.n_games, 6_000_000)
                if self._ref is not None
                else float("nan")
            )
            self.logger.record("selfplay/winrate_vs_random", wr_rand)
            self.logger.record("selfplay/winrate_vs_initial", wr_init)
            if self.verbose:
                print(f"  [{self.num_timesteps}] vs_random={wr_rand:.2f} vs_initial={wr_init:.2f}")
        return True


class LadderEval(BaseCallback):
    """%-trained checkpoint ladder: win-rate of the *current* policy vs its OWN frozen snapshots at
    fixed training-fraction milestones (10/25/50/75% of the budget). Unlike vs-random (which
    saturates fast), this is a non-saturating, self-relative progress curve — a still-improving
    policy keeps beating its earlier selves. Each milestone reads **NaN** until that fraction of
    training is reached (expected/handled, shown as a gap in TB), then logs `ladder/winrate_vs_NNpct`.
    """

    def __init__(self, deck, total_timesteps, eval_freq, n_envs, save_dir,
                 milestones=(0.10, 0.25, 0.50, 0.75), n_games=40, verbose=0):
        super().__init__(verbose)
        self.deck = deck
        self.total = max(int(total_timesteps), 1)
        self.every = max(eval_freq // n_envs, 1)
        self.save_dir = save_dir
        self.milestones = milestones
        self.n_games = n_games
        self._snap = {}  # pct -> ModelOpponent frozen at that milestone

    def _on_training_start(self) -> None:
        os.makedirs(self.save_dir, exist_ok=True)

    def _maybe_snapshot(self):
        for m in self.milestones:
            pct = int(round(m * 100))
            if pct not in self._snap and self.num_timesteps >= m * self.total:
                path = os.path.join(self.save_dir, f"ladder_{pct:02d}")
                self.model.save(path)  # freeze the policy AS IT IS at this training fraction
                self._snap[pct] = ModelOpponent(path + ".zip")

    def _on_step(self) -> bool:
        self._maybe_snapshot()
        if self.n_calls % self.every == 0:
            for m in self.milestones:
                pct = int(round(m * 100))
                wr = (
                    play_winrate(self.model, self.deck, self._snap[pct], self.n_games, 7_000_000 + pct)
                    if pct in self._snap
                    else float("nan")  # not reached yet → NaN (a gap in TB, not an error)
                )
                self.logger.record(f"ladder/winrate_vs_{pct:02d}pct", wr)
        return True


def _clean(*paths):
    for p in paths:
        for f in glob.glob(p):
            try:
                os.remove(f)
            except OSError:
                pass


def train_selfplay(deck="demo", timesteps=120_000, n_envs=8, pool_dir=DEFAULT_POOL,
                   tensorboard_log=None, seed=0, pool_every=8000, eval_every=8000, subproc=False,
                   shaping_coef=0.1, shape_anneal=True, notes=None, replay_every=0, run_name=None,
                   vecenv="batched", num_workers=8, verbose=0,
                   policy=RelationalPointerPolicy, policy_kwargs=None, p_script=0.0, script_mix=None,
                   eval_ladder=True, ent_coef=0.01, learning_rate=3e-4, n_epochs=10, batch_size=256):
    # replay_every>0 records one greedy self-play game every that-many steps to data/replays/ (the
    # lobby's "AI Training Replays" learning progression). Default 0 (OFF) as a library call — the
    # CLI turns it on (~25k) so real runs record, but tests / ab_shaping stay clean.
    # Heuristic reward shaping (potential-based, GYM_PLAN §5) is ON by default: coef0=0.1, CARD-
    # DOMINANT Φ (0.5·cards/0.3·power/0.2·life — see _phi_batch), held full to 50% of training then
    # linearly annealed to 0 across 50%→80% by `ShapingAnneal`. coef 0.1 keeps the cumulative shaping
    # (PBRS telescopes to ≈coef·(Φ_end−Φ_start), |Φ|≤1 ⇒ ≤~0.2) clearly subordinate to the ±1 terminal.
    # PBRS is policy-invariant, so the final policy optimizes only the true ±1 (eval is always on that).
    # Pass shaping_coef=0 to disable. See `ab_shaping.py` for the shaped-vs-unshaped harness.
    os.makedirs(pool_dir, exist_ok=True)
    ref_path = os.path.join(os.path.dirname(pool_dir.rstrip("/")) or ".", "mtgenv_ref_initial.zip")
    ladder_dir = pool_dir.rstrip("/") + "_ladder"
    _clean(os.path.join(pool_dir, "*.zip"), ref_path, os.path.join(ladder_dir, "*.zip"))  # fresh league

    venv = make_vecenv(deck, pool_dir, n_envs, seed, subproc=subproc, vecenv=vecenv,
                       num_workers=num_workers, p_script=p_script, script_mix=script_mix)
    # Default policy = the relational-attention pointer policy (mean-pool EntityExtractor is RETIRED;
    # it can't learn the v3 contract and is a null arch per the user). A caller can still opt into the
    # retired baseline by passing policy="MultiInputPolicy" + features_extractor_class=EntityExtractor.
    # RelationalPointerPolicy builds its own extractor, so no default policy_kwargs are needed.
    pk = policy_kwargs
    model = MaskablePPO(
        policy,
        venv,
        policy_kwargs=pk,
        n_steps=256,
        batch_size=batch_size,
        n_epochs=n_epochs,
        learning_rate=learning_rate,
        gamma=0.999,
        ent_coef=ent_coef,
        seed=seed,
        tensorboard_log=tensorboard_log,
        verbose=verbose,
    )

    # Seed the pool + the eval reference with the initial (random-init) policy.
    model.save(ref_path[:-4])
    model.save(os.path.join(pool_dir, "ckpt_000000000"))

    from mtgenv_gym.tracked_stats import TrackedStatsCallback
    from mtgenv_gym.tb_meta import GameLengthCallback, RunMetadataCallback
    from mtgenv_gym.evalkit import EvalkitCallback

    config = dict(deck=deck, timesteps=timesteps, n_envs=n_envs, seed=seed,
                  shaping_coef0=shaping_coef,
                  shaping_anneal_frac=(0.6 if (shaping_coef > 0 and shape_anneal) else None),
                  learning_rate="3e-4 (SB3 default)", n_steps=256, batch_size=256, gamma=0.999,
                  ent_coef=0.01, pool_every=pool_every, eval_every=eval_every, max_pool=12,
                  p_random=0.2, vec_env="BatchedSelfPlayVecEnv")
    rname = run_name or f"{deck}-{timesteps // 1000}k"
    callbacks = [
        PoolCheckpoint(pool_dir, pool_every, n_envs, max_pool=12, verbose=verbose),
        # evalkit's drop-in eval hook unifies the three legacy eval callbacks (SelfPlayEval +
        # LadderEval + ReplayCheckpoint) behind the algorithm-agnostic Arena — tag-set-identical:
        # selfplay/winrate_vs_random + _vs_initial (+ the new _sampled variants), the %-trained
        # 10/25/50/75% ladder (same seed bases), and periodic greedy self-play replays
        # (replay_every=0 disables). Behaviour stats (stats/*) + game length (game/*) stay owned by
        # TrackedStats/GameLength on the training rollout stream (no double-logging here).
        EvalkitCallback(deck, total_timesteps=timesteps, eval_freq=eval_every, n_envs=n_envs,
                        ref_path=ref_path, ladder_dir=ladder_dir, n_games=40,
                        milestones=((0.10, 0.25, 0.50, 0.75) if eval_ladder else ()),
                        replay_every=replay_every, run_name=rname, verbose=verbose),
        TrackedStatsCallback(),  # action-rate summary stats → stats/* (#68)
        GameLengthCallback(),    # game/turns_mean + end-reason mix (batched env bypasses Monitor)
        RunMetadataCallback(config, notes=notes),  # run/notes text + Custom Scalars dashboard
    ]
    if shaping_coef > 0:
        from mtgenv_gym.batched_selfplay import ShapingAnneal

        if shape_anneal:
            callbacks.append(ShapingAnneal(timesteps, coef0=shaping_coef))  # full to 50%, decay 50->80%
        else:
            # Constant coefficient, no anneal: anneal_start=1.0 holds coef0 for the whole run (the
            # callback still SETS the vec env's shaping_coef=coef0, which make_vecenv leaves at 0).
            callbacks.append(ShapingAnneal(timesteps, coef0=shaping_coef, anneal_start=1.0, anneal_end=1.0))
    # run_name (a versioned `<major>.<minor>-<slug>`, set by main()) names the TB run dir too, so the
    # TB run and the lobby replay tag match. tensorboard_log is the ROOT; SB3 makes <root>/<run_name>_N.
    model.learn(total_timesteps=timesteps, callback=callbacks, progress_bar=False,
                tb_log_name=(run_name or "MaskablePPO"))
    # Persist the FINAL weights into the run dir so a run can be rated/reloaded even after the ephemeral
    # /tmp pool is wiped (the 4.4/4.3 no-recoverable-weights loss must never recur). rate_agent.py adds
    # <run_dir>/final_model.zip. Best-effort — never let a save failure lose the trained model object.
    run_dir = getattr(model.logger, "dir", None) or (model.logger.get_dir() if model.logger else None)
    if run_dir:
        try:
            model.save(os.path.join(run_dir, "final_model"))
            if verbose:
                print(f"saved final weights → {os.path.join(run_dir, 'final_model.zip')}")
        except Exception as e:  # pragma: no cover
            print(f"[selfplay_train] WARN final_model save failed: {e}")
    return model, ref_path


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="demo", choices=["lands", "demo", "burn_vs_bears", "selesnya", "heralds", "bears", "swine"])
    ap.add_argument("--timesteps", type=int, default=120_000)
    ap.add_argument("--n-envs", type=int, default=8)
    ap.add_argument("--pool-dir", default=DEFAULT_POOL)
    ap.add_argument("--tensorboard", default="/home/xander/dev/p-mtg/mtgenv/data/tb", help="TB ROOT; runs land as <root>/<version>-<slug>")
    ap.add_argument("--subproc", action="store_true", help="SubprocVecEnv (parallel workers)")
    ap.add_argument("--shaping-coef", type=float, default=0.1,
                    help="potential-based reward-shaping coef0 (annealed to 0); 0 disables. On by default.")
    ap.add_argument("--no-shape-anneal", action="store_true",
                    help="hold --shaping-coef CONSTANT the whole run (default: full to 50%%, decay to 0 by 80%%)")
    ap.add_argument("--notes", required=True,
                    help="REQUIRED: what this run tests → TB 'run/notes' (TEXT tab) + the Aim run "
                         "description (lifted by scripts/tb2aim.py)")
    ap.add_argument("--replay-every", type=int, default=25_000,
                    help="record one greedy self-play game every N steps → data/replays/ (lobby's AI "
                         "Training Replays). 0 disables. On by default so every run is watchable.")
    ap.add_argument("--run-name", default=None, help="override the full run name (else versioned '<M>.<m>-<deck>-<steps>k')")
    ap.add_argument("--run-major", type=int, default=None,
                    help="bump the TB/replay version major (sticky via <tb-root>/.run_major); minor auto-increments")
    ap.add_argument("--vecenv", default="fleet", choices=["fleet", "batched"],
                    help="'fleet' (M3.4, worker-thread parallel stepping, ~2.8x — DEFAULT) or 'batched' "
                         "(single-threaded Python pump, fallback)")
    ap.add_argument("--num-workers", type=int, default=8, help="fleet worker threads (--vecenv fleet)")
    ap.add_argument("--eval-every", type=int, default=8000, help="steps between the evalkit battery")
    ap.add_argument("--pool-every", type=int, default=8000, help="steps between self-play pool snapshots")
    ap.add_argument("--no-ladder", action="store_true",
                    help="skip the %%-trained ladder eval (superseded by Elo ratings; ~+23%% throughput)")
    ap.add_argument("--p-script", type=float, default=0.0,
                    help="fraction of self-play episodes played vs a punisher heuristic (0 = off)")
    ap.add_argument("--script-mix", default="gang,careful,turtle",
                    help="comma list of ScriptedHeuristic variants for --p-script (racer/turtle/gang/careful)")
    # capacity + optimizer knobs for the campaign (bigger nets + hyperparameter perturbations).
    ap.add_argument("--ent-coef", type=float, default=0.01, help="PPO entropy coefficient (exploration)")
    ap.add_argument("--lr", type=float, default=3e-4, help="PPO learning rate")
    ap.add_argument("--n-epochs", type=int, default=10, help="PPO epochs per rollout")
    ap.add_argument("--batch-size", type=int, default=256, help="PPO minibatch size")
    ap.add_argument("--fe-hidden", type=int, default=64, help="EntityExtractor row-MLP hidden (widen for capacity)")
    ap.add_argument("--fe-features", type=int, default=128, help="EntityExtractor features_dim")
    ap.add_argument("--net-arch", default="64,64", help="actor/critic MLP widths, comma list (e.g. 256,256)")
    args = ap.parse_args()

    # Versioned run name (shared by the TB run dir + the lobby replay tag), unless --run-name overrides.
    from mtgenv_gym.tb_meta import versioned_run_name
    run_name = versioned_run_name(args.tensorboard, f"{args.deck}-{args.timesteps // 1000}k",
                                  major=args.run_major, override=args.run_name)

    # Build policy_kwargs only when a capacity knob differs from the baseline default (else None →
    # train_selfplay's default EntityExtractor + SB3 default net_arch, preserving 4.4-4.6 behaviour).
    pk = None
    if (args.fe_hidden, args.fe_features, args.net_arch) != (64, 128, "64,64"):
        pk = dict(features_extractor_class=EntityExtractor,
                  features_extractor_kwargs=dict(hidden=args.fe_hidden, features_dim=args.fe_features),
                  net_arch=[int(x) for x in args.net_arch.split(",")])

    model, ref = train_selfplay(
        deck=args.deck, timesteps=args.timesteps, n_envs=args.n_envs, pool_dir=args.pool_dir,
        tensorboard_log=args.tensorboard, subproc=args.subproc, shaping_coef=args.shaping_coef,
        shape_anneal=not args.no_shape_anneal,
        notes=args.notes, replay_every=args.replay_every, run_name=run_name,
        # attn default; the EntityExtractor capacity knobs (--fe-*) opt into the retired mean-pool baseline.
        policy=("MultiInputPolicy" if pk else RelationalPointerPolicy),
        vecenv=args.vecenv, num_workers=args.num_workers, verbose=1, policy_kwargs=pk,
        ent_coef=args.ent_coef, learning_rate=args.lr, n_epochs=args.n_epochs, batch_size=args.batch_size,
        eval_every=args.eval_every, pool_every=args.pool_every, eval_ladder=not args.no_ladder,
        p_script=args.p_script, script_mix=(args.script_mix.split(",") if args.p_script > 0 else None),
    )
    wr_rand = play_winrate(model, args.deck, "random", 200, 9_000_000)
    wr_init = play_winrate(model, args.deck, ModelOpponent(ref), 200, 9_500_000)
    print(f"\nfinal: win-rate vs random {wr_rand:.3f} | vs initial self {wr_init:.3f}")
    print(f"pool: {len(glob.glob(os.path.join(args.pool_dir, 'ckpt_*.zip')))} checkpoints")


if __name__ == "__main__":
    main()
