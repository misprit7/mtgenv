"""Explicit engine search agent (SEARCH_PLAN.md S1): Monte-Carlo rollout search over PyGame.fork().

At each COMBAT decision (DeclareAttackers / DeclareBlockers — the judgment the 4.x/5.x campaign
showed is learned worst), every legal candidate is evaluated by forking the live game, applying
the candidate, and rolling the fork to TERMINAL with the base policy net playing both seats
(sampled); Q̂(a) = mean outcome over `--rollouts` rollouts. Every other decision plays the base
net directly (greedy). The engine is the model — no learned dynamics, no value bootstrap (v0
distrusts the value head by design; it was trained on the passive distribution).

⚠ ORACLE CAVEAT (SEARCH_PLAN.md §3): fork() replays the true seed, so rollouts see the real
hidden state. Eval-only in the current pools; never a training-target generator.

Usage:
  python search_agent.py --deck swine --games 20 --opponent careful --ckpt data/tb/5.0-attn-v3_1/final_model.zip
  python search_agent.py --spine --games-per-seat 40 --ckpt ...   # full spine pairing sweep
"""

from __future__ import annotations

import argparse
import json
import time

import numpy as np

import mtg_py
from mtgenv_gym.evalkit.scripted import ScriptedHeuristic, ScriptedPolicy

SPEC = {name: (rows, cols) for (name, rows, cols, _i) in mtg_py.PyGame.obs_spec()}
DECISION_ONEHOT_OFF, NUM_REQUESTS = 43, 21
SEARCH_KINDS = ("DeclareAttackers", "DeclareBlockers")

SCRIPTS = {
    "racer": ("all", "never"),
    "turtle": ("never", "all"),
    "gang": ("all", "gang"),
    "careful": ("conservative", "gang"),
}


def obs_arrays(obs):
    """Flat-list obs dict → correctly-shaped float arrays (what the SB3 policy expects)."""
    out = {}
    for name, (rows, cols) in SPEC.items():
        a = np.asarray(obs[name])
        out[name] = a.reshape(rows, cols) if rows > 1 else a.reshape(-1)
    return out


class NetPolicy:
    """Base checkpoint wrapper: batched-of-one predict with mask, greedy or sampled."""

    def __init__(self, ckpt_path):
        from sb3_contrib import MaskablePPO

        self.model = MaskablePPO.load(ckpt_path, device="cpu")

    def act(self, obs, mask, *, greedy):
        o = {k: v[None, ...] for k, v in obs_arrays(obs).items()}
        a, _ = self.model.predict(o, action_masks=np.asarray(mask, dtype=bool)[None, :],
                                  deterministic=greedy)
        return int(a[0])


class ScriptOpp:
    def __init__(self, kind):
        if kind == "random":
            self.pol = None
        elif kind == "script":
            self.pol = ScriptedPolicy()
        else:
            atk, blk = SCRIPTS[kind]
            self.pol = ScriptedHeuristic(atk, blk)
        self.rng = np.random.default_rng(0xC0FFEE)

    def act(self, obs, mask):
        m = np.asarray(mask, dtype=bool)
        if self.pol is None:
            return int(self.rng.choice(np.flatnonzero(m)))
        return int(self.pol.act([obs_arrays(obs)], [m])[0])


def request_of(tup):
    return tup[3]


class SearchPolicy:
    """The v0 searcher. `act` needs the LIVE PyGame (to fork) — hence the custom game loop below
    rather than the evalkit Arena (whose Policy seam passes only obs/mask)."""

    def __init__(self, net: NetPolicy, rollouts=2, max_candidates=8, max_rollout_decisions=120,
                 rng_seed=0):
        self.net = net
        self.rollouts = rollouts
        self.max_candidates = max_candidates
        self.max_rollout_decisions = max_rollout_decisions
        self.rng = np.random.default_rng(rng_seed)
        self.searched = self.changed = 0  # how often search overrode the greedy net choice

    def _rollout(self, fork, me: int) -> float:
        """Play the fork to terminal with the net on BOTH seats (sampled). Returns 1/0/0.5."""
        tup = fork.step_to_decision()
        for _ in range(self.max_rollout_decisions):
            if tup[5]:
                break
            fork.apply(self.net.act(tup[0], tup[1], greedy=False))
            tup = fork.step_to_decision()
        if not tup[5]:
            return 0.5  # rollout horizon hit without termination — count as a draw
        w = fork.outcome()
        return 0.5 if w is None else float(w == me)

    def act(self, game, tup) -> int:
        obs, mask, seat, req, _nl, _t = tup
        m = np.asarray(mask, dtype=bool)
        legal = np.flatnonzero(m)
        if len(legal) == 1:
            return int(legal[0])
        base = self.net.act(obs, m, greedy=True)
        if req not in SEARCH_KINDS:
            return base
        # MC rollout search over (a capped set of) candidates. The base action always included.
        cands = list(legal)
        if len(cands) > self.max_candidates:
            keep = set(self.rng.choice(cands, size=self.max_candidates - 1, replace=False).tolist())
            keep.add(base)
            cands = sorted(keep)
        me = int(seat)
        q = {}
        for a in cands:
            wins = 0.0
            for _ in range(self.rollouts):
                f = game.fork()
                f.apply(int(a))
                wins += self._rollout(f, me)
            q[a] = wins / self.rollouts
        best = max(q, key=lambda a: (q[a], a == base))  # tie → prefer the net's own choice
        self.searched += 1
        self.changed += best != base
        return int(best)


def play_game(deck, seed, searcher: SearchPolicy, opp: ScriptOpp, my_seat=0):
    g = mtg_py.PyGame(deck, True, False, 0)
    tup = g.reset(seed)
    n_dec = 0
    while not tup[5] and n_dec < 3000:
        n_dec += 1
        if int(tup[2]) == my_seat:
            a = searcher.act(g, tup)
        else:
            a = opp.act(tup[0], tup[1])
        g.apply(int(a))
        tup = g.step_to_decision()
    w = g.outcome()
    return None if w is None else int(w == my_seat)


def run_pairing(deck, opponent, games, searcher, seed0):
    """games total, seats alternating (even seeds: searcher = seat 0)."""
    w = l = d = 0
    for k in range(games):
        opp = ScriptOpp(opponent)
        res = play_game(deck, seed0 + k, searcher, opp, my_seat=k % 2)
        if res is None:
            d += 1
        elif res:
            w += 1
        else:
            l += 1
    return w, l, d


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--deck", default="swine")
    ap.add_argument("--ckpt", required=True)
    ap.add_argument("--rollouts", type=int, default=2)
    ap.add_argument("--max-candidates", type=int, default=8)
    ap.add_argument("--games", type=int, default=20)
    ap.add_argument("--opponent", default="careful",
                    choices=["random", "script", "racer", "turtle", "gang", "careful"])
    ap.add_argument("--spine", action="store_true", help="sweep all 5 spine opponents")
    ap.add_argument("--games-per-seat", type=int, default=40)
    ap.add_argument("--seed0", type=int, default=910_000)
    ap.add_argument("--json-out", default=None)
    args = ap.parse_args()

    net = NetPolicy(args.ckpt)
    searcher = SearchPolicy(net, rollouts=args.rollouts, max_candidates=args.max_candidates)
    t0 = time.time()
    results = {}
    if args.spine:
        for i, opp in enumerate(["random", "racer", "turtle", "gang", "careful"]):
            g = args.games_per_seat * 2
            w, l, d = run_pairing(args.deck, opp, g, searcher, args.seed0 + i * 10_000)
            results[opp] = {"w": w, "l": l, "d": d, "n": g}
            print(f"  vs {opp}: {w}-{l}" + (f"-{d}d" if d else "") + f" (n={g})", flush=True)
    else:
        w, l, d = run_pairing(args.deck, args.opponent, args.games, searcher, args.seed0)
        results[args.opponent] = {"w": w, "l": l, "d": d, "n": args.games}
        print(f"  vs {args.opponent}: {w}-{l}" + (f"-{d}d" if d else "") + f" (n={args.games})")
    dt = time.time() - t0
    print(f"searched {searcher.searched} combat decisions; search overrode the net on "
          f"{searcher.changed} ({searcher.changed / max(searcher.searched, 1):.2f})")
    print(f"wall {dt / 60:.1f} min")
    if args.json_out:
        with open(args.json_out, "w") as f:
            json.dump({"results": results, "searched": searcher.searched,
                       "changed": searcher.changed, "rollouts": args.rollouts,
                       "ckpt": args.ckpt, "wall_s": dt}, f, indent=2)


if __name__ == "__main__":
    main()
