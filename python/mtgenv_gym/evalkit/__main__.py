"""Offline evalkit CLI — backfill / one-off evaluation for ANY algorithm.

    python -m mtgenv_gym.evalkit --run <tb-run-dir> --checkpoint <path> --algo <adapter> \
        --deck <deck> --games N [--step S] [--vs-initial <ckpt>] [--greedy-only] [--no-replay]

Loads the checkpoint via a named **policy adapter**, plays the standard battery (greedy AND sampled),
and appends the canonical TB scalars + a JSON artifact + one replay into ``--run`` (so a backfilled
point overlays the live curves). Generalizes the MuZero backfill scripts.

Adapters are pluggable so a new algorithm registers its loader without evalkit depending on its stack:

    from mtgenv_gym.evalkit.__main__ import register_adapter
    register_adapter("muzero", lambda path, device="cpu": MyMuZeroPolicy(path, device))
"""

from __future__ import annotations

import argparse
import os
import re
from typing import Callable

from .hooks import evaluate_checkpoint
from .policy import RandomPolicy

# name -> (checkpoint_path, *, device) -> Policy
POLICY_ADAPTERS: "dict[str, Callable[..., object]]" = {}


def register_adapter(name: str, fn: "Callable[..., object]") -> None:
    """Register (or override) a policy adapter: ``fn(checkpoint_path, device=...) -> Policy``."""
    POLICY_ADAPTERS[name] = fn


def _sb3_adapter(path, device="cpu"):
    from .sb3 import SB3Policy

    return SB3Policy.load(path, device=device)


def _random_adapter(path=None, device="cpu"):
    return RandomPolicy(seed=0)


register_adapter("sb3", _sb3_adapter)
register_adapter("random", _random_adapter)


def _parse_step(path: "str | None") -> int:
    """Recover a TB x-axis step from a checkpoint name (``ckpt_000012345`` / ``iteration_5`` → int)."""
    if not path:
        return 0
    base = os.path.basename(path)
    m = re.search(r"iteration_(\d+)", base) or re.search(r"(\d+)", base)
    return int(m.group(1)) if m else 0


def main(argv=None) -> None:
    ap = argparse.ArgumentParser(prog="python -m mtgenv_gym.evalkit",
                                 description="Offline evalkit backfill / one-off evaluation.")
    ap.add_argument("--run", required=True, help="TB run dir to append eval scalars + JSON into")
    ap.add_argument("--checkpoint", default=None, help="policy checkpoint path")
    ap.add_argument("--algo", default="sb3", help=f"policy adapter ({', '.join(sorted(POLICY_ADAPTERS))})")
    ap.add_argument("--deck", required=True)
    ap.add_argument("--games", type=int, default=100)
    ap.add_argument("--step", type=int, default=None, help="TB step (default: parse from checkpoint name)")
    ap.add_argument("--vs-initial", default=None,
                    help="also eval vs this checkpoint → selfplay/winrate_vs_initial (loaded via --algo)")
    ap.add_argument("--greedy-only", action="store_true", help="skip the sampled eval")
    ap.add_argument("--no-replay", action="store_true")
    ap.add_argument("--device", default="cpu")
    args = ap.parse_args(argv)

    if args.algo not in POLICY_ADAPTERS:
        ap.error(f"unknown --algo {args.algo!r}; known: {', '.join(sorted(POLICY_ADAPTERS))}")
    adapter = POLICY_ADAPTERS[args.algo]
    policy = adapter(args.checkpoint, device=args.device)
    step = args.step if args.step is not None else _parse_step(args.checkpoint)

    opponents = None
    if args.vs_initial:
        opponents = {
            "selfplay/winrate_vs_random": RandomPolicy(seed=5_000_000),
            "selfplay/winrate_vs_initial": adapter(args.vs_initial, device=args.device),
        }
    modes = ("greedy",) if args.greedy_only else ("greedy", "sample")

    os.environ.setdefault("EVALKIT_VERBOSE", "1")
    res = evaluate_checkpoint(policy, step, args.run, deck=args.deck, opponents=opponents,
                              games=args.games, modes=modes, record_replay=not args.no_replay,
                              algo=args.algo.upper(), run_name=os.path.basename(args.run.rstrip("/")))
    print(f"\nwrote eval @ step {step} → {args.run}")
    for tag, by_mode in res.items():
        print(f"  {tag}: " + "  ".join(f"{m}={r.win_rate:.3f}" for m, r in by_mode.items()))


if __name__ == "__main__":
    main()
