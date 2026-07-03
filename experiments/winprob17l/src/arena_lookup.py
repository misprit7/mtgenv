"""Build arena_id -> (is_creature, power, toughness, cmc) lookup from the local
Scryfall sqlite. Uses front-face characteristics for split/DFC cards. Prefers the
SOS printing when an arena_id collides across printings."""
import sqlite3
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
SQLITE = REPO / "data" / "scryfall" / "cards.sqlite"


def _num(s):
    """Parse a Scryfall power/toughness/cmc string to float; non-numeric (*, X, '') -> 0.0"""
    if s is None:
        return 0.0
    s = str(s).strip()
    try:
        return float(s)
    except ValueError:
        return 0.0  # '*', 'X', '1+*', etc. -> 0 for aggregate purposes


def build_lookup(sqlite_path=SQLITE):
    con = sqlite3.connect(str(sqlite_path))
    rows = con.execute(
        "SELECT arena_id, name, type_line, cmc, power, toughness, set_code "
        "FROM cards WHERE arena_id IS NOT NULL AND arena_id != ''"
    ).fetchall()
    con.close()
    lookup = {}          # arena_id(str) -> dict
    is_sos = {}          # arena_id -> bool (whether current entry is the sos printing)
    for arena_id, name, type_line, cmc, power, toughness, set_code in rows:
        aid = str(arena_id).strip()
        tl = type_line or ""
        front = tl.split("//")[0]  # front face for split/DFC
        entry = {
            "name": name,
            "is_creature": "Creature" in front,
            "power": _num(power),
            "toughness": _num(toughness),
            "cmc": _num(cmc),
        }
        sos = (set_code == "sos")
        # keep SOS printing if we have a collision; else first seen
        if aid not in lookup or (sos and not is_sos.get(aid, False)):
            lookup[aid] = entry
            is_sos[aid] = sos
    return lookup


if __name__ == "__main__":
    lk = build_lookup()
    print(f"arena_id lookup entries: {len(lk)}")
    # spot check
    for aid in ["102521", "102599", "102740", "102497", "102511"]:
        print(aid, lk.get(aid))
