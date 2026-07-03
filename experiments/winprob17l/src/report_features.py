"""Build report_data/features.json — the "what actually matters" visual: win rate vs
binned feature *differentials* (me - opp) at turn checkpoints. Shows the advantage gap
widening into a win-rate gradient as the game develops.

`turn` in the feature table is the ply index, which in MTG == the turn number (P1 turn 1,
P2 turn 2, P1 turn 3, ...), so checkpoints 5/8/11 read naturally.
"""
import json
from pathlib import Path
import numpy as np
import pandas as pd

HERE = Path(__file__).resolve().parent
FEATS = HERE.parent / "data" / "features_SOS_PremierDraft.parquet"
OUT = HERE.parent / "report_data" / "features.json"

CHECKPOINTS = [5, 8, 11]
MIN_N = 80  # drop bins thinner than this from the payload

# count-diff features: integer bins clamped to [-CAP, CAP] (ends are "<= -CAP" / ">= CAP")
COUNT_FEATS = {
    "lands_diff": ("my_lands", "opp_lands", 4),
    "hand_diff": ("my_hand", "opp_hand", 4),
    "creature_diff": ("my_creatures", "opp_creatures", 4),
}
# life diff: width-5 bins clamped to [-15, 15]
LIFE_EDGES = [-100, -10, -5, -0.5, 0.5, 5, 10, 100]
LIFE_LABEL = ["<=-10", "-10..-5", "-5..0", "0", "0..5", "5..10", ">=10"]
LIFE_X = [-12, -7, -2, 0, 2, 7, 12]


def count_series(sub, my, opp, cap):
    diff = (sub[my] - sub[opp]).clip(-cap, cap).round().astype(int)
    won = sub["won"].values
    out = []
    for b in range(-cap, cap + 1):
        m = (diff == b).values
        if m.sum() < MIN_N:
            continue
        label = (f"<={-cap}" if b == -cap else f">={cap}" if b == cap else f"{b:+d}")
        out.append({"bin": label, "x": int(b), "win_rate": round(float(won[m].mean()), 4),
                    "n": int(m.sum())})
    return out


def life_series(sub):
    diff = (sub["my_life"] - sub["opp_life"]).values
    idx = np.digitize(diff, LIFE_EDGES) - 1
    won = sub["won"].values
    out = []
    for b in range(len(LIFE_LABEL)):
        m = idx == b
        if m.sum() < MIN_N:
            continue
        out.append({"bin": LIFE_LABEL[b], "x": LIFE_X[b],
                    "win_rate": round(float(won[m].mean()), 4), "n": int(m.sum())})
    return out


def main():
    df = pd.read_parquet(FEATS)
    feats = {k: {} for k in list(COUNT_FEATS) + ["life_diff"]}
    turn_counts = {}
    for t in CHECKPOINTS:
        sub = df[df["turn"] == t]
        turn_counts[t] = int(len(sub))
        for name, (my, opp, cap) in COUNT_FEATS.items():
            feats[name][str(t)] = count_series(sub, my, opp, cap)
        feats["life_diff"][str(t)] = life_series(sub)

    payload = {
        "checkpoints": CHECKPOINTS,
        "turn_semantics": "turn = ply index = MTG turn number (P1 t1, P2 t2, P1 t3, ...)",
        "rows_per_checkpoint": turn_counts,
        "min_bin_n": MIN_N,
        "features": feats,
        "defs": {
            "lands_diff": "my lands in play - opp lands in play",
            "hand_diff": "my cards in hand - opp cards in hand",
            "creature_diff": "my creatures in play - opp creatures in play",
            "life_diff": "my life - opp life",
        },
    }
    OUT.write_text(json.dumps(payload, indent=2))
    ncells = sum(len(v[str(t)]) for v in feats.values() for t in CHECKPOINTS)
    print(f"features.json: {len(feats)} features x {len(CHECKPOINTS)} checkpoints, "
          f"{ncells} bins; rows/checkpoint {turn_counts}")


if __name__ == "__main__":
    main()
