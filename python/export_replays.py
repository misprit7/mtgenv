"""Export training replays across a **self-play** run so you can *watch the agent learn*
(REPLAY_PLAN §3 + GYM_PLAN §8.2). Trains against a growing pool of frozen selves (the M2 league),
in one continuous run (clean TensorBoard curves), and records one **self-play** game (the current
policy on both seats) at checkpoints to ``data/replays/``, tagged ``AiTraining{step}``, viewable in
the web lobby's "AI Training Replays" section.

    PYTHONPATH=python python python/export_replays.py --deck burn_vs_bears --tensorboard /tmp/mtgenv_tb
    tensorboard --logdir /tmp/mtgenv_tb     # the run appears as MaskablePPO_<n>
"""

from __future__ import annotations

import argparse
import os
import re
import time

import glob

from sb3_contrib import MaskablePPO
from stable_baselines3.common.callbacks import BaseCallback

from mtgenv_gym import MtgEnv
from mtgenv_gym.league import ModelOpponent, PoolCheckpoint
from mtgenv_gym.policy import EntityExtractor
from selfplay_train import make_vecenv, SelfPlayEval, LadderEval

REPLAY_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "data", "replays"))


def _now_ms() -> int:
    return int(time.time() * 1000)


def record_game(model, deck, step, out_dir, run_name=None, seed=12_345, self_play=True):
    """Record one game with the current policy on seat 0. With ``self_play`` the opponent (seat 1)
    is the *same* policy (true self-play — the agent vs itself); otherwise a random opponent."""
    opponent = ModelOpponent(model, deterministic=False) if self_play else "random"
    # Explicit per-game decision cap (defense-in-depth): truncates a between-games non-terminating
    # game to a draw. (It can't catch an in-engine, in-step loop — control never returns to Python —
    # but the recording env should carry the same cap the training env does.)
    env = MtgEnv(deck=deck, record_replay=True, replay_step=step, opponent=opponent, max_decisions=3000)
    obs, info = env.reset(seed=seed + step)
    done = False
    while not done:
        action, _ = model.predict(obs, action_masks=info["action_mask"], deterministic=False)
        obs, _r, term, trunc, info = env.step(int(action))
        done = term or trunc
    sides = deck.split("_vs_") if "_vs_" in deck else [deck, deck]
    tag = f"PPO@{run_name}:{step}" if run_name else f"PPO@{step}"
    opp = tag if self_play else "random"
    try:
        return env.export_replay(out_dir, _now_ms(), names=[tag, opp], decks=sides[:2], run_name=run_name)
    except Exception as e:
        # A replay we can't serialize (e.g. the engine's Selesnya counter-map → "key must be a
        # string", flagged to the engine team) must not kill the training run — skip it, warn once.
        if not getattr(record_game, "_warned", False):
            print(f"  [replay] export skipped ({type(e).__name__}: {e}) — training continues")
            record_game._warned = True
        return None


def _run_name(model) -> str:
    """The TensorBoard run folder, minus SB3's ``_N`` suffix — so the replay run tag matches the
    descriptive name passed as ``tb_log_name`` (e.g. ``demo-selfplay-60k-0614-1432``)."""
    logdir = getattr(getattr(model, "logger", None), "dir", None)
    name = os.path.basename(logdir) if logdir else "run"
    return re.sub(r"_\d+$", "", name)


class ReplayCheckpoint(BaseCallback):
    """Record one replay every ``record_every`` env-steps during a single continuous ``learn()``."""

    def __init__(self, deck, out_dir, record_every, n_envs):
        super().__init__()
        self.deck = deck
        self.out_dir = out_dir
        self.every_calls = max(record_every // n_envs, 1)
        self.run_name = "run"

    def _on_training_start(self) -> None:
        self.run_name = _run_name(self.model)
        # An initial, pre-training (random-policy) checkpoint at step 0.
        record_game(self.model, self.deck, 0, self.out_dir, run_name=self.run_name)

    def _on_step(self) -> bool:
        if self.n_calls % self.every_calls == 0:
            path = record_game(
                self.model, self.deck, self.num_timesteps, self.out_dir, run_name=self.run_name
            )
            if self.verbose and path:
                print(f"  step {self.num_timesteps:>6}: {os.path.basename(path)}")
        return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="demo", choices=["lands", "demo", "burn_vs_bears", "selesnya"])
    ap.add_argument("--timesteps", type=int, default=60_000)
    ap.add_argument("--record-every", type=int, default=10_000, help="record a replay every N steps")
    ap.add_argument("--tensorboard", default="/tmp/mtgenv_tb", help="TensorBoard log dir")
    ap.add_argument("--n-envs", type=int, default=8)
    ap.add_argument("--pool-dir", default="/tmp/mtgenv_pool_export")
    ap.add_argument("--run-name", default=None,
                    help="descriptive run label (else auto: '<deck>-selfplay-<steps>k-<mmdd-HHMM>')")
    ap.add_argument("--shaping-coef", type=float, default=0.5,
                    help="initial potential-based shaping coef (annealed to 0 over 60%% of training; "
                         "0 disables — GYM_PLAN §5)")
    ap.add_argument("--big-net", action="store_true",
                    help="attention-based AttnEntityExtractor + [256,256] heads (~630k params) "
                         "instead of the DeepSets mean-pool baseline (~144k) — A/B for net capacity")
    args = ap.parse_args()

    # Descriptive run name → TensorBoard run folder AND the replay run tag (the lobby groups by it).
    # Pass --run-name to tag what changed between runs (e.g. 'demo-selfplay-deepnet').
    run_name = args.run_name or f"{args.deck}-selfplay-{args.timesteps // 1000}k-{time.strftime('%m%d-%H%M')}"

    # SELF-PLAY: train against a growing pool of frozen selves (not random) — so both the training
    # AND the recorded replays are genuine self-play.
    os.makedirs(args.pool_dir, exist_ok=True)
    for f in glob.glob(os.path.join(args.pool_dir, "*.zip")):
        os.remove(f)
    venv = make_vecenv(args.deck, args.pool_dir, args.n_envs, 0, subproc=False)
    if args.big_net:
        from mtgenv_gym.policy import AttnEntityExtractor

        policy_kwargs = dict(features_extractor_class=AttnEntityExtractor,
                             net_arch=dict(pi=[256, 256], vf=[256, 256]))
        lr = 1e-4  # gentler LR for the attention net (the default 3e-4 NaN'd it)
    else:
        policy_kwargs = dict(features_extractor_class=EntityExtractor)
        lr = 3e-4
    model = MaskablePPO(
        "MultiInputPolicy", venv,
        policy_kwargs=policy_kwargs,
        n_steps=256, batch_size=256, gamma=0.999, ent_coef=0.01, learning_rate=lr,
        tensorboard_log=args.tensorboard, verbose=1,
    )
    # Frozen random-init reference for the vs-initial eval curve. Kept OUTSIDE pool_dir so the
    # OpponentPool (globs pool_dir/*.zip) never samples it and PoolCheckpoint never prunes it.
    ref_path = args.pool_dir.rstrip("/") + "_ref.zip"
    model.save(os.path.join(args.pool_dir, "ckpt_000000000"))  # seed the league
    model.save(ref_path[:-4])
    cbs = [
        PoolCheckpoint(args.pool_dir, max(args.record_every // 2, 4000), args.n_envs, max_pool=12),
        ReplayCheckpoint(args.deck, REPLAY_DIR, args.record_every, args.n_envs),
        # Self-play progress curves: winrate vs random AND vs the initial (random-init) self. The
        # vs-initial curve is the real "self-play is improving" signal — the mirror rollout reward
        # sits at ~0 by symmetry, and vs-random plateaus once the policy beats a weak baseline.
        SelfPlayEval(args.deck, ref_path, max(args.record_every // 2, 4000), args.n_envs, n_games=40),
        # %-trained ladder: current policy vs its own 10/25/50/75%-of-budget snapshots (non-saturating).
        LadderEval(args.deck, args.timesteps, max(args.record_every // 2, 4000), args.n_envs,
                   save_dir=args.pool_dir.rstrip("/") + "_ladder", n_games=40),
    ]
    if args.shaping_coef > 0:
        from mtgenv_gym.batched_selfplay import ShapingAnneal

        cbs.append(ShapingAnneal(args.timesteps, coef0=args.shaping_coef, anneal_frac=0.6))
    for c in cbs:
        c.verbose = 1

    print(f"run={run_name}  deck={args.deck}  timesteps={args.timesteps}  SELF-PLAY  → {REPLAY_DIR}")
    t0 = time.time()
    model.learn(total_timesteps=args.timesteps, callback=cbs, tb_log_name=run_name, progress_bar=False)
    print(f"\ndone in {time.time() - t0:.0f}s — TensorBoard run '{run_name}' under {args.tensorboard}")
    print("replays: lobby 'AI Training Replays' section (mtg-serve on :8080)")


if __name__ == "__main__":
    main()
