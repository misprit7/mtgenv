"""Throughput + exit-criteria report for milestone 0: thousands of random self-play games.

    python benchmark.py --deck demo --games 3000
    python benchmark.py --deck lands --games 5000 --no-auto-pass

Prints games/s, decisions/s, the conservation pass-rate, and the win distribution. This is the
"thousands of legal games/s, no panics, non-empty mask, conservation holds" measurement.
"""

from __future__ import annotations

import argparse
import time

import numpy as np

from mtgenv_gym import play_random_game


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="demo", choices=["lands", "demo", "burn_vs_bears", "selesnya", "heralds", "bears"])
    ap.add_argument("--games", type=int, default=3000)
    ap.add_argument("--auto-pass", dest="auto_pass", action="store_true", default=True)
    ap.add_argument("--no-auto-pass", dest="auto_pass", action="store_false")
    args = ap.parse_args()

    rng = np.random.default_rng(0)
    decisions = 0
    empty_mask_games = 0
    cards_ok = 0
    zones_ok = 0
    winners = {0: 0, 1: 0, None: 0}
    reasons: dict[str, int] = {}

    t0 = time.perf_counter()
    for s in range(args.games):
        st = play_random_game(deck=args.deck, seed=s, auto_pass=args.auto_pass, rng=rng)
        decisions += st.decisions
        empty_mask_games += int(st.min_legal < 1)
        cards_ok += int(st.cards_conserved)
        zones_ok += int(st.zones_conserved)
        winners[st.winner] = winners.get(st.winner, 0) + 1
        reasons[st.reason] = reasons.get(st.reason, 0) + 1
    dt = time.perf_counter() - t0

    print(f"deck={args.deck}  auto_pass={args.auto_pass}  games={args.games}")
    print(f"  wall                {dt:8.3f} s")
    print(f"  games/s             {args.games / dt:8.1f}")
    print(f"  decisions           {decisions:8d}  ({decisions / dt:,.0f}/s, {decisions / args.games:.1f}/game)")
    print(f"  empty-mask games    {empty_mask_games:8d}  (must be 0)")
    print(f"  cards conserved     {cards_ok}/{args.games}")
    print(f"  zones conserved     {zones_ok}/{args.games}")
    print(f"  winners             seat0={winners.get(0,0)} seat1={winners.get(1,0)} draw={winners.get(None,0)}")
    print(f"  end reasons         {reasons}")

    ok = empty_mask_games == 0 and cards_ok == args.games and zones_ok == args.games
    print(f"  EXIT CRITERIA       {'PASS' if ok else 'FAIL'}")


if __name__ == "__main__":
    main()
