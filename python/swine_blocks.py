"""Chump-blocking-at-high-life analyzer (swine experiment).

The user's observation: the swine policy chump-blocks the trampling 3/3 Swine even at high life
(where you should just take 3 rather than trade a 2/2 into a trampler for nothing). This plays the
trained policy against its own greedy copy and, at every DeclareBlockers decision, records the
blocker's life and the block SHAPE (did it block at all? did it gang / double-block, the sophisticated
anti-trample play?). Bucketing by life (>=15 vs <15) exposes whether it still blocks when not under
pressure. Compares against the bears control (no trample -> single-blocking is fine there).

    PYTHONPATH=python python/.venv/bin/python python/swine_blocks.py \
        --model <pool_or_ladder>.zip --deck swine --games 300

Reads block stats from `info["decision_stats"]` (decision_stats.rs: block_eligible/block_declared/
attackers_blocked/block_double) and my_life from obs["globals"][16]. Greedy (deterministic) throughout.
"""

from __future__ import annotations

import argparse
import glob
import os

import numpy as np

from mtgenv_gym import MtgEnv
from mtgenv_gym.league import ModelOpponent

_MY_LIFE = 16  # globals index (matches batched_selfplay._G_MY_LIFE)
# bf_feat per-permanent columns (obs.rs): [0]=present [1]=is_mine [2]=power; the last four are
# attacking / blocking / is_src / is_cand — so attacking = width-4. A swine is the only 3-power
# creature in the deck (bears are 2/2), so an ENEMY attacking creature with power==3 is a trampler.
_BF_PRESENT, _BF_MINE, _BF_POWER = 0, 1, 2


def _swine_attacking(bf):
    """True if an ENEMY (is_mine=0) creature with power==3 is currently attacking (a trampling Swine)."""
    atk = bf.shape[1] - 4
    m = (bf[:, _BF_PRESENT] > 0.5) & (bf[:, _BF_MINE] < 0.5) & (bf[:, _BF_POWER] == 3) & (bf[:, atk] > 0.5)
    return bool(m.any())


def newest_checkpoint(pool_dir: str) -> str:
    zips = glob.glob(os.path.join(pool_dir, "*.zip"))
    if not zips:
        raise SystemExit(f"no checkpoints in {pool_dir}")
    return max(zips, key=os.path.getmtime)


def analyze(model_path: str, deck: str, games: int, life_hi: int, seed0: int = 3_000_000):
    from sb3_contrib import MaskablePPO

    model = MaskablePPO.load(model_path, device="cpu")
    # Opponent = a greedy copy of the same policy, so the attacks the blocker faces are purposeful.
    env = MtgEnv(deck=deck, opponent=ModelOpponent(model_path))

    # rows: (my_life, blocked?, block_double?, attackers_blocked, swine_attacking?)
    rows = []
    for i in range(games):
        obs, info = env.reset(seed=seed0 + i)
        done = False
        while not done:
            life = float(obs["globals"][_MY_LIFE])
            swine = _swine_attacking(obs["bf_feat"])  # from the obs we're about to act on
            action, _ = model.predict(obs, action_masks=info["action_mask"], deterministic=True)
            obs, _r, term, trunc, info = env.step(int(action))
            done = term or trunc
            st = info.get("decision_stats")
            if st and "block_eligible" in st and st["block_eligible"] > 0:
                rows.append((life, 1.0 if st["block_declared"] > 0 else 0.0,
                             1.0 if st["block_double"] > 0 else 0.0,
                             st["attackers_blocked"], 1.0 if swine else 0.0))
    return np.array(rows) if rows else np.empty((0, 5))


def _line(name, r):
    if len(r) == 0:
        print(f"  {name:38s}: (none)")
        return
    block_rate = r[:, 1].mean()          # fraction of decisions where it declared >=1 blocker
    gang_rate = r[r[:, 1] > 0, 2].mean() if (r[:, 1] > 0).any() else float("nan")  # of blocks, frac double
    atk_blocked = r[:, 3].mean()
    print(f"  {name:38s}: n={len(r):4d}  block_rate={block_rate:.3f}  "
          f"gang_rate|blocked={gang_rate:.3f}  atk_blocked={atk_blocked:.2f}")


def report(rows, deck, life_hi):
    print(f"\n=== {deck}: {len(rows)} DeclareBlockers decisions (greedy self-mirror) ===")
    if len(rows) == 0:
        print("  (no block decisions — opponent never attacked into an eligible blocker)")
        return
    life, swine = rows[:, 0], rows[:, 4] > 0.5
    hi = life >= life_hi
    print("  -- by life --")
    _line(f"life >= {life_hi} (no pressure)", rows[hi])
    _line(f"life <  {life_hi} (under pressure)", rows[~hi])
    print("  -- gated on a SWINE (3/3 trampler) attacking --")
    _line(f"SWINE attacking & life >= {life_hi}", rows[swine & hi])   # the user's exact concern
    _line(f"SWINE attacking & life <  {life_hi}", rows[swine & ~hi])
    _line("NO swine (bears only)", rows[~swine])
    _line("ALL", rows)
    print(f"  → CHUMP signal: high block_rate in 'SWINE attacking & life>={life_hi}' with low gang_rate "
          "= single-blocking the trampler when it should take the 3.")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default=None, help="checkpoint .zip (default: newest in --pool-dir)")
    ap.add_argument("--pool-dir", default="/tmp/mtgenv_swine500/pool")
    ap.add_argument("--deck", default="swine")
    ap.add_argument("--games", type=int, default=300)
    ap.add_argument("--life-hi", type=int, default=15, help="'high life' threshold (>= is 'no pressure')")
    args = ap.parse_args()

    model_path = args.model or newest_checkpoint(args.pool_dir)
    print(f"analyzing {model_path} on {args.deck}, {args.games} greedy self-mirror games")
    rows = analyze(model_path, args.deck, args.games, args.life_hi)
    report(rows, args.deck, args.life_hi)


if __name__ == "__main__":
    main()
