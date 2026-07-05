"""SWINE combat-judgment analysis on a trained MuZero checkpoint — the original question.

PPO's known failure: at life ≥ 15 it still chump-blocks the trampling 3/3 Swine ~94-97% (should take 3),
and rarely gang-blocks (the correct anti-trample play). This measures the SAME on greedy (fair-tie-break)
MuZero play, in a self-mirror so the attacks the blocker faces are purposeful. Mirrors
python/swine_blocks.py's protocol + eval_muzero_swine.py's block_analysis, but drives the fair-greedy
MZAgent (argmax visits, ties by prior) instead of the low-index-biased raw argmax.

Run: PYTHONPATH=../../python .venv/bin/python swine_judgment.py --model <ckpt> --latent 256 --games 300
"""
from __future__ import annotations

import argparse
import numpy as np
import torch

import lz_patches  # noqa
from muzero_metrics import build_policy, _obs_keys
from muzero_observability import MZAgent
from mtgenv_gym import MtgEnv

_MY_LIFE = 16
_BF_PRESENT, _BF_MINE, _BF_POWER = 0, 1, 2


def _swine_attacking(bf):
    atk = bf.shape[1] - 5
    m = ((bf[:, _BF_PRESENT] > 0.5) & (bf[:, _BF_MINE] < 0.5) & (bf[:, _BF_POWER] == 3) & (bf[:, atk] > 0.5))
    return bool(m.any())


def block_analysis(agent, deck, games, life_hi=15, seed0=3_000_000):
    """Greedy self-mirror; per DeclareBlockers decision record (life, blocked?, gang?, #blocked, swine?)."""
    env = MtgEnv(deck=deck, opponent=agent)  # opponent = same fair-greedy MuZero policy
    rows = []
    for i in range(games):
        obs, info = env.reset(seed=seed0 + i)
        done = False
        while not done:
            life = float(obs["globals"][_MY_LIFE])
            swine = _swine_attacking(obs["bf_feat"])
            a = agent.act_greedy(obs, info["action_mask"])
            obs, _r, term, trunc, info = env.step(int(a))
            done = term or trunc
            st = info.get("decision_stats")
            if st and st.get("block_eligible", 0) > 0:
                rows.append((life,
                             1.0 if st["block_declared"] > 0 else 0.0,
                             1.0 if st["block_double"] > 0 else 0.0,
                             float(st["attackers_blocked"]),
                             1.0 if swine else 0.0))
    return np.array(rows) if rows else np.empty((0, 5))


def _line(name, r):
    if len(r) == 0:
        print(f"  {name:34s}: (none)"); return
    block_rate = r[:, 1].mean()
    gang = r[r[:, 1] > 0, 2].mean() if (r[:, 1] > 0).any() else float("nan")
    print(f"  {name:34s}: n={len(r):4d}  block_rate={block_rate:.3f}  gang|blocked={gang:.3f}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", required=True)
    ap.add_argument("--config", default="swine_plain")
    ap.add_argument("--deck", default="swine")
    ap.add_argument("--games", type=int, default=300)
    ap.add_argument("--life-hi", type=int, default=15)
    ap.add_argument("--latent", type=int, default=256)
    ap.add_argument("--sims", type=int, default=None)
    ap.add_argument("--device", default="cuda" if torch.cuda.is_available() else "cpu")
    args = ap.parse_args()

    policy, _ = build_policy(args.config, args.model, args.device, latent=args.latent, sims=args.sims)
    agent = MZAgent(policy, _obs_keys(args.deck), args.device)
    rows = block_analysis(agent, args.deck, args.games, args.life_hi)
    print(f"\n=== {len(rows)} DeclareBlockers decisions (greedy fair self-mirror), {args.model} ===")
    if len(rows) == 0:
        print("  (no block decisions — policy may not reach combat)"); return
    life, swine = rows[:, 0], rows[:, 4] > 0.5
    hi = life >= args.life_hi
    _line(f"SWINE atk & life>={args.life_hi} (THE test)", rows[swine & hi])
    _line(f"SWINE atk & life< {args.life_hi}", rows[swine & ~hi])
    _line("NO swine (bears in view)", rows[~swine])
    _line("ALL", rows)
    print(f"  PPO baseline: chump-block ~0.94-0.97 @ life>=15, gang ~0.15. Lower block_rate / higher gang = better judgment.")


if __name__ == "__main__":
    main()
