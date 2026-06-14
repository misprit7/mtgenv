"""Self-play training (GYM_PLAN §8.2): train a MaskablePPO policy against a growing pool of frozen
copies of itself (a self-play league), on the demo deck (mirror). Logs win-rate vs random and vs
the initial (random-init) checkpoint to TensorBoard — "stable self-play improvement" is the M2 exit
signal (the policy keeps beating its past selves while staying strong vs random).

    PYTHONPATH=python python python/selfplay_train.py --timesteps 120000 --tensorboard /tmp/mtgenv_tb
    tensorboard --logdir /tmp/mtgenv_tb

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


def make_vecenv(deck, pool_dir, n_envs, seed, subproc=False, batched=True, p_random=0.2, device=None):
    """Self-play vec env. Default = ``BatchedSelfPlayVecEnv`` (#41): all games stepped in lockstep
    with opponent inference batched across games (~1.2–1.4× at n_envs 32–64, scales with n_envs and
    net size). ``batched=False`` falls back to the legacy ``DummyVecEnv`` of per-env ``OpponentPool``
    envs (one synchronous opponent ``predict`` each). ``subproc`` only applies to the legacy path."""
    if batched:
        from mtgenv_gym import BatchedSelfPlayVecEnv

        if device is None:
            import torch

            device = "cuda" if torch.cuda.is_available() else "cpu"
        return BatchedSelfPlayVecEnv(deck, pool_dir, n_envs, p_random=p_random, seed=seed,
                                     device=device)
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
                   shaping_coef=0.0, verbose=0):
    os.makedirs(pool_dir, exist_ok=True)
    ref_path = os.path.join(os.path.dirname(pool_dir.rstrip("/")) or ".", "mtgenv_ref_initial.zip")
    _clean(os.path.join(pool_dir, "*.zip"), ref_path)  # fresh league each run

    venv = make_vecenv(deck, pool_dir, n_envs, seed, subproc=subproc)
    model = MaskablePPO(
        "MultiInputPolicy",
        venv,
        policy_kwargs=dict(features_extractor_class=EntityExtractor),
        n_steps=256,
        batch_size=256,
        gamma=0.999,
        ent_coef=0.01,
        seed=seed,
        tensorboard_log=tensorboard_log,
        verbose=verbose,
    )

    # Seed the pool + the eval reference with the initial (random-init) policy.
    model.save(ref_path[:-4])
    model.save(os.path.join(pool_dir, "ckpt_000000000"))

    from mtgenv_gym.tracked_stats import TrackedStatsCallback

    callbacks = [
        PoolCheckpoint(pool_dir, pool_every, n_envs, max_pool=12, verbose=verbose),
        SelfPlayEval(deck, ref_path, eval_every, n_envs, verbose=verbose),
        TrackedStatsCallback(),  # action-rate summary stats → stats/* (#68)
    ]
    if shaping_coef > 0:
        from mtgenv_gym.batched_selfplay import ShapingAnneal

        callbacks.append(ShapingAnneal(timesteps, coef0=shaping_coef, anneal_frac=0.6))
    model.learn(total_timesteps=timesteps, callback=callbacks, progress_bar=False)
    return model, ref_path


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="demo", choices=["lands", "demo", "burn_vs_bears", "selesnya"])
    ap.add_argument("--timesteps", type=int, default=120_000)
    ap.add_argument("--n-envs", type=int, default=8)
    ap.add_argument("--pool-dir", default=DEFAULT_POOL)
    ap.add_argument("--tensorboard", default=None)
    ap.add_argument("--subproc", action="store_true", help="SubprocVecEnv (parallel workers)")
    args = ap.parse_args()

    model, ref = train_selfplay(
        deck=args.deck, timesteps=args.timesteps, n_envs=args.n_envs, pool_dir=args.pool_dir,
        tensorboard_log=args.tensorboard, subproc=args.subproc, verbose=1,
    )
    wr_rand = play_winrate(model, args.deck, "random", 200, 9_000_000)
    wr_init = play_winrate(model, args.deck, ModelOpponent(ref), 200, 9_500_000)
    print(f"\nfinal: win-rate vs random {wr_rand:.3f} | vs initial self {wr_init:.3f}")
    print(f"pool: {len(glob.glob(os.path.join(args.pool_dir, 'ckpt_*.zip')))} checkpoints")


if __name__ == "__main__":
    main()
