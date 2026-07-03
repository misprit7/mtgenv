"""Parse the 17lands SOS PremierDraft replay CSV into a generic-feature table.

One output row per (game, end-of-turn snapshot). Perspective is always the data
collector ("user" = me, "oppo" = opp). Features are card-identity-free (life, counts,
per-creature P/T/CMC aggregates) so the same vector is computable later from our engine's
PlayerView. Label = the game's `won` (constant within a game -> split BY GAME).

See ../README.md "Feature spec" for the single source of truth.
"""
import sys
from pathlib import Path
import numpy as np
import pandas as pd

sys.path.insert(0, str(Path(__file__).resolve().parent))
from arena_lookup import build_lookup

HERE = Path(__file__).resolve().parent
DATA = HERE.parent / "data"
REPLAY_GZ = DATA / "replay_SOS_PremierDraft.csv.gz"
OUT = DATA / "features_SOS_PremierDraft.parquet"

MAX_TURN = 30
GAME_COLS = ["on_play", "num_turns", "won", "num_mulligans", "opp_num_mulligans"]

# per-turn eot column templates (side in {user, oppo} = me/opp)
EOT_TMPL = [
    "eot_{side}_life",
    "eot_{side}_cards_in_hand",
    "eot_{side}_lands_in_play",
    "eot_{side}_creatures_in_play",
    "eot_{side}_non_creatures_in_play",
]


def turn_cols(fam, n):
    """All eot column names for family fam ('user'/'oppo') at turn n, both sides."""
    cols = {}
    for side in ("user", "oppo"):
        for t in EOT_TMPL:
            key = t.format(side=side)
            cols[(side, key)] = f"{fam}_turn_{n}_{key}"
    return cols


def build_usecols():
    cols = list(GAME_COLS)
    for fam in ("user", "oppo"):
        for n in range(1, MAX_TURN + 1):
            cols += list(turn_cols(fam, n).values())
    return cols


def parse_id_list(v):
    """A pipe-delimited arena_id field -> list[str]. Handles NaN, single value."""
    if v is None:
        return []
    if isinstance(v, float) and np.isnan(v):
        return []
    s = str(v).strip()
    if s == "" or s.lower() == "nan":
        return []
    return [x for x in s.split("|") if x != ""]


def creature_aggs(ids, lookup):
    """Aggregate P/T/CMC over the creature arena_ids in `ids`."""
    powers, tous, cmcs = [], [], []
    for aid in ids:
        e = lookup.get(aid)
        if e is None or not e["is_creature"]:
            continue
        powers.append(e["power"])
        tous.append(e["toughness"])
        cmcs.append(e["cmc"])
    n = len(powers)
    if n == 0:
        return dict(pow_sum=0.0, tou_sum=0.0, pow_max=0.0, cmc_sum=0.0, cmc_mean=0.0)
    return dict(
        pow_sum=float(np.sum(powers)),
        tou_sum=float(np.sum(tous)),
        pow_max=float(np.max(powers)),
        cmc_sum=float(np.sum(cmcs)),
        cmc_mean=float(np.mean(cmcs)),
    )


def snapshot_features(row, fam, n, lookup):
    """Return a feature dict for the eot snapshot of family fam at turn n, or None if
    that snapshot is padding. Caller must only pass n in 1..num_turns; absent turns are
    padded with life=0.0 / board=NaN, so we also drop the both-players-at-0 padding
    signature as a safety guard (a real snapshot never has both players at exactly 0)."""
    c = turn_cols(fam, n)
    life_me = row.get(c[("user", "eot_user_life")])
    if life_me is None or (isinstance(life_me, float) and np.isnan(life_me)) or str(life_me).strip() in ("", "nan"):
        return None
    try:
        my_life = float(life_me)
        opp_life = float(row.get(c[("oppo", "eot_oppo_life")]))
    except (TypeError, ValueError):
        return None
    if my_life == 0.0 and opp_life == 0.0:
        return None  # padding beyond the real game length

    my_hand_ids = parse_id_list(row.get(c[("user", "eot_user_cards_in_hand")]))
    # opp hand is a count field, not an id list
    opp_hand_raw = row.get(c[("oppo", "eot_oppo_cards_in_hand")])
    try:
        opp_hand = float(opp_hand_raw)
        if np.isnan(opp_hand):
            opp_hand = 0.0
    except (TypeError, ValueError):
        opp_hand = float(len(parse_id_list(opp_hand_raw)))

    my_lands = parse_id_list(row.get(c[("user", "eot_user_lands_in_play")]))
    opp_lands = parse_id_list(row.get(c[("oppo", "eot_oppo_lands_in_play")]))
    my_creat = parse_id_list(row.get(c[("user", "eot_user_creatures_in_play")]))
    opp_creat = parse_id_list(row.get(c[("oppo", "eot_oppo_creatures_in_play")]))
    my_nonc = parse_id_list(row.get(c[("user", "eot_user_non_creatures_in_play")]))
    opp_nonc = parse_id_list(row.get(c[("oppo", "eot_oppo_non_creatures_in_play")]))

    ma = creature_aggs(my_creat, lookup)
    oa = creature_aggs(opp_creat, lookup)

    on_play = 1 if bool(row["on_play"]) else 0
    my_turn = 1 if fam == "user" else 0
    # global ply ordering (turn feature)
    if on_play:
        ply = 2 * n - 1 if fam == "user" else 2 * n
    else:
        ply = 2 * n if fam == "user" else 2 * n - 1

    return dict(
        turn=ply,
        my_turn=my_turn,
        on_play=on_play,
        my_life=my_life,
        opp_life=opp_life,
        my_hand=float(len(my_hand_ids)),
        opp_hand=opp_hand,
        my_lands=float(len(my_lands)),
        opp_lands=float(len(opp_lands)),
        my_creatures=float(len(my_creat)),
        opp_creatures=float(len(opp_creat)),
        my_noncreatures=float(len(my_nonc)),
        opp_noncreatures=float(len(opp_nonc)),
        my_pow_sum=ma["pow_sum"], opp_pow_sum=oa["pow_sum"],
        my_tou_sum=ma["tou_sum"], opp_tou_sum=oa["tou_sum"],
        my_pow_max=ma["pow_max"], opp_pow_max=oa["pow_max"],
        my_cmc_sum=ma["cmc_sum"], opp_cmc_sum=oa["cmc_sum"],
        my_cmc_mean=ma["cmc_mean"], opp_cmc_mean=oa["cmc_mean"],
    )


TOTAL_GAMES = 582914  # counted from the file (win_rate 0.554, avg 9.25 turns)


def sampled_index_set(sample_size, seed):
    """Deterministic set of global game indices to include (None = all)."""
    if not sample_size or sample_size >= TOTAL_GAMES:
        return None
    rng = np.random.default_rng(seed)
    idx = rng.choice(TOTAL_GAMES, size=sample_size, replace=False)
    return set(int(i) for i in idx)


def main(sample_size=80000, seed=17, max_games=None):
    lookup = build_lookup()
    usecols = build_usecols()
    keep = sampled_index_set(sample_size, seed)
    reader = pd.read_csv(
        REPLAY_GZ, usecols=lambda c: c in set(usecols),
        dtype=str, chunksize=20000, low_memory=False,
    )
    frames = []          # per-chunk DataFrames (bounds peak memory)
    global_idx = 0       # index over ALL games in file
    n_games = 0          # games actually parsed (sampled)
    n_won = 0
    for chunk in reader:
        chunk_rows = []
        for _, row in chunk.iterrows():
            gi = global_idx
            global_idx += 1
            if keep is not None and gi not in keep:
                continue
            won = str(row["won"]).strip().lower() in ("true", "1", "1.0")
            on_play = str(row["on_play"]).strip().lower() in ("true", "1", "1.0")
            r = row.to_dict()
            r["on_play"] = on_play
            try:
                num_turns = int(float(row["num_turns"]))
            except (TypeError, ValueError):
                num_turns = MAX_TURN
            last_turn = min(num_turns, MAX_TURN)
            n_games += 1
            n_won += int(won)
            for fam in ("user", "oppo"):
                for n in range(1, last_turn + 1):
                    feat = snapshot_features(r, fam, n, lookup)
                    if feat is None:
                        continue
                    feat["game_id"] = gi
                    feat["won"] = int(won)
                    chunk_rows.append(feat)
            if max_games and n_games >= max_games:
                break
        if chunk_rows:
            frames.append(pd.DataFrame(chunk_rows))
        print(f"  ...scanned {global_idx} games, sampled {n_games}, "
              f"{sum(len(f) for f in frames)} rows", flush=True)
        if max_games and n_games >= max_games:
            break

    df = pd.concat(frames, ignore_index=True)
    df.to_parquet(OUT, index=False)
    print(f"sampled_games={n_games} won_rate={n_won/max(n_games,1):.4f} rows={len(df)} -> {OUT}")
    print(df.describe().T[["mean", "min", "max"]].to_string())


if __name__ == "__main__":
    ss = int(sys.argv[1]) if len(sys.argv) > 1 else 80000
    main(sample_size=ss)
