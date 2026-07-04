"""PLUMBING AUDIT step 1e — step through a game's opening sub-decisions BY HAND, focusing on the
mulligan loop, printing per sub-step: request kind, legal mask, chosen action, reward, done.

The failure mode to catch: a mask/obs desync or a spurious non-terminal reward at the mulligan
sub-decisions (the always-mulligan collapse could point here). We force the "always mulligan"
line to see exactly what the learner experiences when it does what the collapsed policy does.
"""
from __future__ import annotations

import numpy as np
from mtgenv_gym import MtgEnv

# The engine surfaces a `request` string per decision (info['request']). Print it so we can see
# which sub-steps are mulligan decisions vs mana/main/etc.


def walk(deck="swine", seed=7, force_mulligan=True, max_steps=40):
    env = MtgEnv(deck=deck, opponent="random")
    obs, info = env.reset(seed=seed)
    print(f"\n===== HAND-WALK deck={deck} seed={seed} force_mulligan={force_mulligan} =====")
    rng = np.random.default_rng(seed)
    for t in range(max_steps):
        mask = np.asarray(info["action_mask"], dtype=bool)
        legal = np.flatnonzero(mask)
        req = info.get("request", "?")
        seat = info.get("seat", "?")
        # Heuristic: identify the mulligan decision. The engine's request string names it.
        req_s = str(req)
        is_mull = "mull" in req_s.lower() or "keep" in req_s.lower()
        # choose action: if forcing mulligan and this looks like a mulligan decision, pick the
        # "mulligan" option; else random legal.
        chosen = None
        note = ""
        if force_mulligan and is_mull and legal.size >= 2:
            # We don't know which index is "mulligan" vs "keep" without decoding; print both and
            # take the first legal (documented below after we see the decode).
            chosen = int(legal[0])
            note = "(mull-decision: took legal[0])"
        else:
            chosen = int(rng.choice(legal))
        obs2, reward, term, trunc, info = env.step(chosen)
        stats = info.get("decision_stats")
        print(f"  t={t:2d} seat={seat} req={req_s:28s} n_legal={legal.size:2d} legal={legal.tolist()[:8]}"
              f"{'...' if legal.size>8 else '':3s} -> a={chosen:2d} r={reward:+.1f} term={int(term)} trunc={int(trunc)}"
              f"  {note}")
        if stats:
            # show only nonzero fields
            nz = {k: v for k, v in stats.items() if abs(float(v)) > 1e-9}
            if nz:
                print(f"        decision_stats(nonzero)={nz}")
        obs = obs2
        if term or trunc:
            print(f"  --> TERMINAL at t={t}: reward={reward:+.1f}, summary={info.get('summary')}")
            break


if __name__ == "__main__":
    import sys
    deck = sys.argv[1] if len(sys.argv) > 1 else "swine"
    # A couple of seeds; one forcing the mulligan line, one random.
    walk(deck=deck, seed=7, force_mulligan=True)
    walk(deck=deck, seed=7, force_mulligan=False)
