"""Build report_data/headline.json + meta.json from the FULL replay file's game-level
columns (all 582,914 games — cheap, only a handful of columns).

headline.json: dataset scale, collector win rate, on-play vs on-draw, game-length
distribution (win rate + count by num_turns), mulligan rate + win-rate-by-mulligans.
"""
import json
from pathlib import Path
import numpy as np
import pandas as pd

HERE = Path(__file__).resolve().parent
DATA = HERE.parent / "data"
REPLAY_GZ = DATA / "replay_SOS_PremierDraft.csv.gz"
OUT = HERE.parent / "report_data"

# from Phase 1 (documented in ../README.md); the sampled feature table's row count
SAMPLE_GAMES = 80000
SAMPLE_ROWS = 1437392


def _bool(s):
    return s.astype(str).str.strip().str.lower().isin(["true", "1", "1.0"])


def main():
    cols = ["won", "on_play", "num_turns", "num_mulligans", "opp_num_mulligans"]
    tot = 0
    won = 0
    # accumulators
    onplay = {"play": [0, 0], "draw": [0, 0]}          # [games, wins]
    by_turns = {}                                       # num_turns -> [games, wins]
    by_mull = {}                                        # num_mulligans -> [games, wins]
    for ch in pd.read_csv(REPLAY_GZ, usecols=cols, dtype=str, chunksize=50000):
        w = _bool(ch["won"]).values
        op = _bool(ch["on_play"]).values
        nt = pd.to_numeric(ch["num_turns"], errors="coerce").fillna(-1).astype(int).values
        nm = pd.to_numeric(ch["num_mulligans"], errors="coerce").fillna(-1).astype(int).values
        tot += len(ch)
        won += int(w.sum())
        onplay["play"][0] += int(op.sum()); onplay["play"][1] += int((w & op).sum())
        onplay["draw"][0] += int((~op).sum()); onplay["draw"][1] += int((w & ~op).sum())
        for t in np.unique(nt):
            if t < 0:
                continue
            m = nt == t
            g, wi = by_turns.setdefault(int(t), [0, 0])
            by_turns[int(t)] = [g + int(m.sum()), wi + int((w & m).sum())]
        for k in np.unique(nm):
            if k < 0:
                continue
            m = nm == k
            g, wi = by_mull.setdefault(int(k), [0, 0])
            by_mull[int(k)] = [g + int(m.sum()), wi + int((w & m).sum())]

    def wr(gw):
        g, wi = gw
        return round(wi / g, 4) if g else None

    # game-length series (cap the long tail into a 20+ bucket for the viz)
    length_series = []
    tail_g = tail_w = 0
    for t in sorted(by_turns):
        g, wi = by_turns[t]
        if t >= 20:
            tail_g += g; tail_w += wi
            continue
        length_series.append({"num_turns": t, "games": g, "win_rate": wr([g, wi])})
    if tail_g:
        length_series.append({"num_turns": "20+", "games": tail_g, "win_rate": wr([tail_g, tail_w])})

    mull_series = [{"mulligans": k, "games": by_mull[k][0], "win_rate": wr(by_mull[k])}
                   for k in sorted(by_mull) if by_mull[k][0] >= 50]
    n_kept = sum(by_mull[k][0] for k in by_mull if k >= 1)

    headline = {
        "total_games": tot,
        "collector_win_rate": round(won / tot, 4),
        "sample_games": SAMPLE_GAMES,
        "snapshot_rows": SAMPLE_ROWS,
        "on_play_vs_draw": {
            "on_play": {"games": onplay["play"][0], "win_rate": wr(onplay["play"])},
            "on_draw": {"games": onplay["draw"][0], "win_rate": wr(onplay["draw"])},
        },
        "game_length": length_series,   # win rate + count by num_turns (collector's turns)
        "mulligans": {
            "kept_all_seven_rate": round(1 - n_kept / tot, 4),
            "by_mulligans": mull_series,  # win rate vs # mulligans taken
        },
    }
    (OUT / "headline.json").write_text(json.dumps(headline, indent=2))
    print(f"headline.json: {tot} games, win {headline['collector_win_rate']}, "
          f"{len(length_series)} length bins, {len(mull_series)} mull bins")

    meta = {
        "set": "SOS", "set_name": "Secrets of Strixhaven",
        "format": "PremierDraft", "source": "17lands.com public datasets (CC BY 4.0)",
        "files": {
            "replay_data": "replay_data_public.SOS.PremierDraft.csv.gz",
            "game_data": "game_data_public.SOS.PremierDraft.csv.gz",
        },
        "date_pulled": "2026-07-03",
        "total_games": tot,
        "feature_sample_games": SAMPLE_GAMES,
        "feature_snapshot_rows": SAMPLE_ROWS,
        "card_data_source": "game_data (GIH/OH/GP win rates)",
        "note": "Generic-feature model + per-card rankings; card P/T/CMC/name joined from local Scryfall sqlite.",
    }
    (OUT / "meta.json").write_text(json.dumps(meta, indent=2))
    print("meta.json written")


if __name__ == "__main__":
    main()
