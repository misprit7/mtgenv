#!/usr/bin/env python3
"""Regenerate card-art.json — the baked-in card manifest (grp_id -> art_crop/normal/artist/set).

The card list is NOT hand-maintained: it comes from the engine itself via the `dump-cards` binary
(every registered card). So whenever a card is added, re-running this picks it up automatically —
there's nothing to keep in sync by hand.

Two data sources:
  - **Art** (art_crop / normal / artist): the Scryfall API (`/cards/collection`), whichever printing
    it returns — art is art.
  - **`set`** (the card's first-printing set code): the LOCAL Scryfall sqlite
    (`data/scryfall/cards.sqlite`), the earliest paper `released_at` per name, excluding
    promo/token/etc. printings. This matches the repo's per-set-folder convention (a card's folder =
    its first real printing) rather than whatever printing the art API happened to return, and gives
    the lobby deck-builder a stable set code to filter/search on.

Usage:  python3 crates/mtg-gre-server/resolve-card-art.py
        (run from anywhere; needs network for art, and data/scryfall/cards.sqlite for set codes)
Then rebuild so the new manifest is baked in:  cargo build -p mtg-gre-server
"""
import json
import os
import sqlite3
import subprocess
import sys

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
OUT_PATH = os.path.join(REPO_ROOT, "crates", "mtg-gre-server", "card-art.json")
CARDS_DB = os.path.join(REPO_ROOT, "data", "scryfall", "cards.sqlite")

# set_types that are NOT a card's "real" first printing — promos/tokens/etc. can predate the
# expansion and would otherwise mislabel the set (e.g. Bonesplitter's earliest row is a 2003 promo,
# not Mirrodin). Excluding these makes the earliest-released row match the repo's per-set folder.
_NON_PRINTING_SET_TYPES = ("token", "promo", "memorabilia", "minigame", "vanguard", "treasure_chest")


def load_sets(names):
    """name -> first-printing set code, from the local Scryfall sqlite (earliest paper `released_at`,
    excluding promo/token/etc.). Names with no matching real printing (e.g. pure tokens) are omitted."""
    if not os.path.exists(CARDS_DB):
        print(f"  note: {os.path.relpath(CARDS_DB, REPO_ROOT)} missing — no set codes emitted")
        return {}
    db = sqlite3.connect(CARDS_DB)
    placeholders = ",".join("?" * len(_NON_PRINTING_SET_TYPES))
    # Match the exact name, or a double-faced card whose full Scryfall name is "<front> // <back>"
    # (the engine stores only the front-face name).
    query = (
        f"SELECT set_code FROM cards WHERE (name = ? OR name LIKE ?) AND digital = '0' "
        f"AND set_type NOT IN ({placeholders}) ORDER BY released_at ASC LIMIT 1"
    )
    out = {}
    for n in names:
        row = db.execute(query, (n, n + " // %", *_NON_PRINTING_SET_TYPES)).fetchone()
        if row:
            out[n] = row[0]
    db.close()
    return out


def load_card_pool():
    """grp_id -> exact Scryfall name, sourced from `cargo run --bin dump-cards` (the engine's decks)."""
    proc = subprocess.run(
        ["cargo", "run", "-q", "-p", "mtg-gre-server", "--bin", "dump-cards"],
        cwd=REPO_ROOT, capture_output=True, text=True,
    )
    if proc.returncode != 0:
        sys.exit(f"dump-cards failed:\n{proc.stderr}")
    return {int(e["grp_id"]): e["name"] for e in json.loads(proc.stdout)}


def fetch_scryfall(names):
    """Batch-resolve names via /cards/collection (<=75 ids per call); name -> card object."""
    byname = {}
    for i in range(0, len(names), 75):
        chunk = names[i:i + 75]
        body = json.dumps({"identifiers": [{"name": n} for n in chunk]})
        out = subprocess.run(
            ["curl", "-s", "-X", "POST", "https://api.scryfall.com/cards/collection",
             "-H", "Content-Type: application/json", "-H", "Accept: application/json",
             "-H", "User-Agent: mtgenv/0.1 (research)", "-d", body],
            capture_output=True, text=True,
        ).stdout
        resp = json.loads(out)
        for c in resp.get("data", []):
            byname[c["name"]] = c
            # Double-faced cards come back under their full "Front // Back" name; also index by the
            # front-face name so lookups by the engine's (front-face-only) card name resolve.
            byname.setdefault(c["name"].split(" // ")[0], c)
        if resp.get("not_found"):
            print("  not found:", [nf.get("name") for nf in resp["not_found"]])
    return byname


def main():
    cards = load_card_pool()
    names = list(cards.values())
    byname = fetch_scryfall(names)
    sets = load_sets(names)  # first-printing set code per name, from the local sqlite
    manifest, missing = {}, []
    for gid, name in sorted(cards.items()):
        c = byname.get(name)
        if not c:
            missing.append(name)
            continue
        # Double-faced cards have no top-level image_uris; fall back to the front face.
        iu = c.get("image_uris") or (c.get("card_faces", [{}])[0].get("image_uris", {}))
        entry = {
            "name": name,
            "art": iu.get("art_crop"),
            "img": iu.get("normal"),
            "artist": c.get("artist"),
        }
        if name in sets:
            entry["set"] = sets[name]
        manifest[str(gid)] = entry
    json.dump(manifest, open(OUT_PATH, "w"), indent=1, ensure_ascii=False)
    n_sets = sum(1 for e in manifest.values() if "set" in e)
    print(f"wrote {len(manifest)} entries to {os.path.relpath(OUT_PATH, REPO_ROOT)} ({n_sets} with a set code)")
    if missing:
        print("MISSING (no Scryfall match — check the exact card name):", missing)


if __name__ == "__main__":
    main()
