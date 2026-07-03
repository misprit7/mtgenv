"""Build report_data/cards.json — per-card rankings from the SOS game_data file.

Standard 17lands card metrics (all over the FULL game_data, streamed):
  GP  WR = win rate in games where the card was in the maindeck   (deck_<C> >= 1)
  OH  WR = win rate in games where it was in the opening hand      (opening_hand_<C> >= 1)
  GIH WR = win rate in games where it was ever in hand             (opening_hand+drawn+tutored >= 1)
Each with its sample size (n_gp / n_oh / n_gih) so the viz can gate on confidence.
Card name / mana_cost / colors / rarity / type joined from the local Scryfall sqlite
(prefer the SOS printing; SOS has a bonus sheet, so fall back to any printing by name).
"""
import json
import sqlite3
from pathlib import Path
import numpy as np
import pandas as pd

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
GAME_GZ = HERE.parent / "data" / "game_SOS_PremierDraft.csv.gz"
SQLITE = REPO / "data" / "scryfall" / "cards.sqlite"
OUT = HERE.parent / "report_data" / "cards.json"

MIN_GP = 200  # drop ultra-thin cards from the ranking payload


def card_meta():
    """name -> metadata. Keyed by full Scryfall name AND by front-face name (game_data
    lists DFC/split cards by their front face only). Prefers the SOS printing on collision."""
    con = sqlite3.connect(str(SQLITE))
    rows = con.execute(
        "SELECT name, set_code, mana_cost, cmc, colors, rarity, type_line FROM cards"
    ).fetchall()
    con.close()
    meta, is_sos = {}, {}
    for name, sc, mana, cmc, colors, rarity, tl in rows:
        sos = (sc == "sos")
        try:
            cmcf = float(cmc) if cmc not in (None, "") else None
        except ValueError:
            cmcf = None
        # scryfall `colors` is a JSON-array string like '["R"]'/'[]' -> compact "R"/"" in WUBRG order
        clr = ""
        if colors:
            picked = [c for c in "WUBRG" if f'"{c}"' in colors]
            clr = "".join(picked)
        entry = {
            "mana_cost": mana or "", "cmc": cmcf,
            "colors": clr, "rarity": rarity or "",
            "type": (tl or "").split("//")[0].strip(),
        }
        for key in {name, name.split("//")[0].strip()}:   # full + front-face alias
            if key not in meta or (sos and not is_sos.get(key, False)):
                meta[key] = entry
                is_sos[key] = sos
    return meta


def main():
    header = pd.read_csv(GAME_GZ, nrows=0)
    cols = list(header.columns)
    fam = {p: {} for p in ("deck_", "drawn_", "opening_hand_", "tutored_")}
    for c in cols:
        for p in fam:
            if c.startswith(p):
                fam[p][c[len(p):]] = c
                break
    # cards present as maindeck columns are the universe
    cards = sorted(fam["deck_"].keys())
    deck_cols = [fam["deck_"][c] for c in cards]
    oh_cols = [fam["opening_hand_"].get(c) for c in cards]
    dr_cols = [fam["drawn_"].get(c) for c in cards]
    tu_cols = [fam["tutored_"].get(c) for c in cards]

    read_cols = ["won"] + deck_cols + [c for c in oh_cols + dr_cols + tu_cols if c]
    read_cols = list(dict.fromkeys(read_cols))

    n_cards = len(cards)
    gp = np.zeros((n_cards, 2))   # [games, wins]
    oh = np.zeros((n_cards, 2))
    gih = np.zeros((n_cards, 2))
    n_games = 0

    dt = {c: "float32" for c in read_cols if c != "won"}
    for ch in pd.read_csv(GAME_GZ, usecols=read_cols, dtype=dt, chunksize=40000):
        won = ch["won"].astype(str).str.strip().str.lower().isin(["true", "1", "1.0"]).values.astype(np.float64)
        n_games += len(ch)

        def block(colnames):
            present = [c for c in colnames]
            arr = np.zeros((len(ch), n_cards), dtype=np.float32)
            for j, c in enumerate(colnames):
                if c is not None and c in ch.columns:
                    arr[:, j] = np.nan_to_num(ch[c].values)
            return arr

        dm = block(deck_cols) >= 1
        ohm = block(oh_cols) >= 1
        gihm = (block(oh_cols) + block(dr_cols) + block(tu_cols)) >= 1
        gp[:, 0] += dm.sum(0); gp[:, 1] += (dm * won[:, None]).sum(0)
        oh[:, 0] += ohm.sum(0); oh[:, 1] += (ohm * won[:, None]).sum(0)
        gih[:, 0] += gihm.sum(0); gih[:, 1] += (gihm * won[:, None]).sum(0)

    meta = card_meta()

    def wr(row):
        return round(row[1] / row[0], 4) if row[0] > 0 else None

    out = []
    for i, name in enumerate(cards):
        if gp[i, 0] < MIN_GP:
            continue
        m = meta.get(name, {})
        out.append({
            "name": name,
            "mana_cost": m.get("mana_cost", ""), "cmc": m.get("cmc"),
            "colors": m.get("colors", ""), "rarity": m.get("rarity", ""),
            "type": m.get("type", ""),
            "gih_wr": wr(gih[i]), "n_gih": int(gih[i, 0]),
            "oh_wr": wr(oh[i]), "n_oh": int(oh[i, 0]),
            "gp_wr": wr(gp[i]), "n_gp": int(gp[i, 0]),
        })
    # rank by GIH WR (with a soft sample floor already applied)
    out.sort(key=lambda d: (d["gih_wr"] if d["gih_wr"] is not None else -1), reverse=True)

    payload = {
        "n_games": n_games,
        "min_gp_filter": MIN_GP,
        "metric_defs": {
            "gih_wr": "win rate in games where the card was ever in hand (opening_hand+drawn+tutored)",
            "oh_wr": "win rate in games with the card in the opening hand",
            "gp_wr": "win rate in games with the card in the maindeck (games played)",
        },
        "cards": out,
    }
    OUT.write_text(json.dumps(payload, separators=(",", ":")))  # compact for inlining
    unmatched = [c["name"] for c in out if not c["type"]]
    print(f"cards.json: {n_games} games, {len(out)} cards (>= {MIN_GP} GP), "
          f"{len(unmatched)} without sqlite meta")
    if unmatched[:8]:
        print("  unmatched sample:", unmatched[:8])
    print("  top-5 GIH WR:", [(c["name"], c["gih_wr"]) for c in out[:5]])


if __name__ == "__main__":
    main()
