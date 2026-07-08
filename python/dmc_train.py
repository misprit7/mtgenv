"""Deep Monte-Carlo (DouZero-style) self-play trainer — the model-free contrast arm.

Trains a single shared action-as-input Q-net by regressing every visited ``(state, action)`` to its
game's final ±1 return (undiscounted Monte-Carlo; no critic, no bootstrapping, shaping OFF), via mirror
self-play. All eval/metrics/logging go through evalkit, so the run overlays on the same TensorBoard
dashboard as the PPO and tree-search arms (identical tag schema).

    PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 PYTHONPATH=python python/.venv/bin/python python/dmc_train.py \
        --deck heralds --env-steps 500000 --notes "DMC heralds arm" --run-name 4.3-dmc-heralds

See ``mtgenv_gym.dmc.train_dmc`` for the loop; this file is the CLI + the human-readable
``<run_dir>/description.md`` artifact.
"""

from __future__ import annotations

import argparse
import os


def write_description(run_dir, args, head="action-as-input"):
    """The human-readable run card (what DMC is, what question this arm answers, key hypers)."""
    os.makedirs(run_dir, exist_ok=True)
    txt = f"""# {args.run_name} — Deep Monte-Carlo (DMC) self-play, deck: {args.deck}

## What DMC is
DouZero-style **Deep Monte-Carlo**: a purely model-free value method. Play full self-play games with an
ε-greedy actor over the *legal* actions; the regression target for **every** `(state, action)` visited
is that game's **final ±1 result** from the acting seat's perspective — undiscounted (γ=1), no
bootstrapping, no critic, no policy gradient. `Q(s,a)` is regressed to that Monte-Carlo return with MSE.
Reward shaping is **OFF** (canonical DMC learns the raw ±1, which is exactly what eval scores).

## The question this arm answers
The 4.0–4.2 arms are **model-based tree search** (LightZero / MuZero: a learned dynamics model + MCTS).
This arm is the **model-free Monte-Carlo** contrast: how far does a plain "regress Q to the final
outcome" learner get on heralds, with no search and no model? It isolates the value of the tree-search
machinery — if DMC reaches the PPO/MuZero band (sustained ≥0.97 greedy vs random on the final
checkpoint), the search apparatus is buying little here; if it plateaus below, that gap is the search
contribution. Compared on **sustained final-checkpoint** win-rate (no peak-picking).

## Head: {head}
MTG's action head is a fixed `Discrete(ACTION_DIM)` of *factored slots* whose meaning is positional (`codec.rs`):
`HAND[i]`/`PERM[i]`/`STACK[i]` slots point at a specific entity ROW already in the observation
(hand/battlefield/stack); `COMMIT`/`PLAYER`/`MODE`/`COLOR`/`NUMBER`/`YES`/`NO` are abstract. So the net
scores `Q(s,a)` from the **content of the object the action points at** (its encoded entity row) — the
DouZero action-as-input design — not from a bare slot index, generalising across which slot a card
occupies. Abstract slots use a learned per-slot embedding. State summary is a DeepSets mean-pool over
the entity tables (same encoder the PPO extractor reads), so the comparison differs in the *learning
rule*, not the observation.

## Key hyperparameters
- env steps (applied sub-steps, both seats): {args.env_steps:,}   eval every {args.eval_every:,} ({args.eval_games} games, greedy + sampled)
- self-play envs: {args.n_envs} (mirror; current net plays both seats; transitions collected for both)
- ε schedule: {args.eps_start} → {args.eps_end} linear over {int(args.eps_decay_frac*100)}% of training
- optimizer: Adam lr={args.lr}; batch {args.batch_size}; {args.updates_per_iter} updates / {args.collect_per_iter} collected; replay buffer {args.buffer_capacity:,}
- γ = 1.0 (undiscounted); shaping OFF; sample-mode temperature {args.sample_temp} (softmax over legal Q — value methods have no native action distribution)
- device: {'cuda' if args.device in (None, 'cuda') else args.device}   seed: {args.seed}

## Notes
{args.notes}
"""
    with open(os.path.join(run_dir, "description.md"), "w") as f:
        f.write(txt)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="heralds",
                    choices=["lands", "demo", "burn_vs_bears", "selesnya", "heralds", "bears", "swine"])
    ap.add_argument("--env-steps", type=int, default=500_000)
    ap.add_argument("--n-envs", type=int, default=64)
    ap.add_argument("--run-name", default="4.3-dmc-heralds", help="TB run dir name (exact)")
    ap.add_argument("--tensorboard", default="/home/xander/dev/p-mtg/mtgenv/data/tb", help="TB ROOT; run lands at <root>/<run-name>")
    ap.add_argument("--eval-every", type=int, default=25_000)
    ap.add_argument("--eval-games", type=int, default=200)
    ap.add_argument("--buffer-capacity", type=int, default=60_000)
    ap.add_argument("--batch-size", type=int, default=512)
    ap.add_argument("--updates-per-iter", type=int, default=16)
    ap.add_argument("--collect-per-iter", type=int, default=2048)
    ap.add_argument("--lr", type=float, default=1e-3)
    ap.add_argument("--eps-start", type=float, default=0.9)
    ap.add_argument("--eps-end", type=float, default=0.05)
    ap.add_argument("--eps-decay-frac", type=float, default=0.6)
    ap.add_argument("--sample-temp", type=float, default=0.1)
    ap.add_argument("--device", default=None, help="cuda | cpu (default: cuda if available)")
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--no-eval-vs-initial", action="store_true")
    ap.add_argument("--notes", required=True,
                    help="REQUIRED: what this run tests → TB 'run/notes' + <run_dir>/description.md")
    args = ap.parse_args()

    from mtgenv_gym.dmc import train_dmc

    run_dir = os.path.join(args.tensorboard, args.run_name)
    write_description(run_dir, args)

    net, run_dir = train_dmc(
        deck=args.deck, env_steps=args.env_steps, n_envs=args.n_envs, run_name=args.run_name,
        tensorboard_root=args.tensorboard, eval_every=args.eval_every, eval_games=args.eval_games,
        buffer_capacity=args.buffer_capacity, batch_size=args.batch_size,
        updates_per_iter=args.updates_per_iter, collect_per_iter=args.collect_per_iter, lr=args.lr,
        eps_start=args.eps_start, eps_end=args.eps_end, eps_decay_frac=args.eps_decay_frac,
        sample_temp=args.sample_temp, device=args.device, seed=args.seed, notes=args.notes,
        eval_vs_initial=not args.no_eval_vs_initial, verbose=1,
    )
    print(f"done: run_dir={run_dir}")


if __name__ == "__main__":
    main()
