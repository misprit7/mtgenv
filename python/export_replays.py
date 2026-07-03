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
import glob
import os
import time

from sb3_contrib import MaskablePPO

from mtgenv_gym.league import PoolCheckpoint
from mtgenv_gym.policy import EntityExtractor
from mtgenv_gym.replays import REPLAY_DIR, ReplayCheckpoint
from mtgenv_gym.tracked_stats import TrackedStatsCallback
from selfplay_train import make_vecenv, SelfPlayEval, LadderEval


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="demo", choices=["lands", "demo", "burn_vs_bears", "selesnya", "heralds", "bears", "swine"])
    ap.add_argument("--timesteps", type=int, default=60_000)
    ap.add_argument("--record-every", type=int, default=10_000, help="record a replay every N steps")
    ap.add_argument("--tensorboard", default="/tmp/mtgenv_tb", help="TensorBoard log dir")
    ap.add_argument("--n-envs", type=int, default=8)
    ap.add_argument("--pool-dir", default="/tmp/mtgenv_pool_export")
    ap.add_argument("--run-name", default=None,
                    help="override the full run name (else versioned '<M>.<m>-<deck>-selfplay-<steps>k')")
    ap.add_argument("--run-major", type=int, default=None,
                    help="bump the TB/replay version major (sticky via <tb-root>/.run_major); minor auto-increments")
    ap.add_argument("--shaping-coef", type=float, default=0.1,
                    help="initial potential-based shaping coef (annealed to 0 over 60%% of training; "
                         "0 disables — GYM_PLAN §5)")
    ap.add_argument("--big-net", action="store_true",
                    help="attention-based AttnEntityExtractor + [256,256] heads (~630k params) "
                         "instead of the DeepSets mean-pool baseline (~144k) — A/B for net capacity")
    ap.add_argument("--greedy", action="store_true",
                    help="record greedy (argmax) games — the same play the greedy diagnostics analyze")
    ap.add_argument("--notes", default=None,
                    help="freeform run description → TensorBoard 'run/notes' (TEXT tab)")
    args = ap.parse_args()

    # Versioned run name (<major>.<minor>-<slug>) → the TB run folder AND the lobby replay tag, so the
    # two correlate 1:1 and sort in run order. --run-name overrides the whole thing.
    from mtgenv_gym.tb_meta import versioned_run_name
    run_name = versioned_run_name(args.tensorboard, f"{args.deck}-selfplay-{args.timesteps // 1000}k",
                                  major=args.run_major, override=args.run_name)

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
    from mtgenv_gym.tb_meta import GameLengthCallback, RunMetadataCallback

    config = dict(deck=args.deck, timesteps=args.timesteps, n_envs=args.n_envs,
                  shaping_coef0=args.shaping_coef, learning_rate=lr, big_net=args.big_net,
                  record_every=args.record_every, greedy_replays=args.greedy, run_name=run_name)
    cbs = [
        PoolCheckpoint(args.pool_dir, max(args.record_every // 2, 4000), args.n_envs, max_pool=12),
        ReplayCheckpoint(args.deck, args.record_every, args.n_envs, out_dir=REPLAY_DIR,
                         run_name=run_name, deterministic=args.greedy),
        RunMetadataCallback(config, notes=args.notes),  # run/notes text + Custom Scalars dashboard
        GameLengthCallback(),                            # game/turns_mean + end-reason mix
        # Self-play progress curves: winrate vs random AND vs the initial (random-init) self. The
        # vs-initial curve is the real "self-play is improving" signal — the mirror rollout reward
        # sits at ~0 by symmetry, and vs-random plateaus once the policy beats a weak baseline.
        SelfPlayEval(args.deck, ref_path, max(args.record_every // 2, 4000), args.n_envs, n_games=40),
        # %-trained ladder: current policy vs its own 10/25/50/75%-of-budget snapshots (non-saturating).
        LadderEval(args.deck, args.timesteps, max(args.record_every // 2, 4000), args.n_envs,
                   save_dir=args.pool_dir.rstrip("/") + "_ladder", n_games=40),
        # Action-rate summary stats (cast/attack/block/playland) to TensorBoard under stats/* (#68).
        TrackedStatsCallback(),
    ]
    if args.shaping_coef > 0:
        from mtgenv_gym.batched_selfplay import ShapingAnneal

        cbs.append(ShapingAnneal(args.timesteps, coef0=args.shaping_coef))  # full to 50%, decay 50->80%
    for c in cbs:
        c.verbose = 1

    print(f"run={run_name}  deck={args.deck}  timesteps={args.timesteps}  SELF-PLAY  → {REPLAY_DIR}")
    t0 = time.time()
    model.learn(total_timesteps=args.timesteps, callback=cbs, tb_log_name=run_name, progress_bar=False)
    print(f"\ndone in {time.time() - t0:.0f}s — TensorBoard run '{run_name}' under {args.tensorboard}")
    print("replays: lobby 'AI Training Replays' section (mtg-serve on :8080)")


if __name__ == "__main__":
    main()
