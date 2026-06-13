#!/usr/bin/env python3
"""Build a queryable SQLite index from Scryfall's default_cards bulk JSON.

The raw bulk file (`data/scryfall/default-cards.json`, ~550MB, one object per
*printing*) is far too large to `jq`/scan per lookup (~2 min/pass). This builds
`data/scryfall/cards.sqlite` — one row per printing, indexed by name and
oracle_id, holding only the gameplay-relevant columns the engine cares about.

Card-authoring then becomes instant, e.g.:

    # gameplay fields for a card
    sqlite3 data/scryfall/cards.sqlite \
      "SELECT name,mana_cost,type_line,power,toughness,oracle_text
         FROM cards WHERE name='Llanowar Elves' LIMIT 1;"

    # the set a card was FIRST printed in (folder organization)
    sqlite3 data/scryfall/cards.sqlite \
      "SELECT set_code FROM cards WHERE name='Llanowar Elves'
         ORDER BY released_at ASC LIMIT 1;"

Streams the array with raw_decode so peak memory is ~the file size, not a
115k-element list of dicts. Idempotent: rebuilds the table from scratch.
"""
import json
import os
import sqlite3
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(REPO_ROOT, "data", "scryfall", "default-cards.json")
DB = os.path.join(REPO_ROOT, "data", "scryfall", "cards.sqlite")

# Columns kept (everything else — prices, image uris, legalities, rulings — dropped).
SCALAR = ["name", "set", "set_name", "set_type", "released_at", "type_line",
          "mana_cost", "cmc", "oracle_text", "power", "toughness", "loyalty",
          "layout", "rarity", "arena_id", "oracle_id", "reprint", "digital"]
# List/struct fields stored as JSON text.
JSONCOL = ["color_identity", "colors", "keywords", "produced_mana", "card_faces"]


def rows(path):
    with open(path, "r", encoding="utf-8") as f:
        data = f.read()
    dec = json.JSONDecoder()
    i = data.index("[") + 1
    n = len(data)
    while True:
        # skip whitespace and commas between elements
        while i < n and data[i] in " \t\r\n,":
            i += 1
        if i >= n or data[i] == "]":
            break
        obj, end = dec.raw_decode(data, i)
        i = end
        yield obj


def main():
    if not os.path.exists(SRC):
        sys.exit(f"error: {SRC} not found — run scripts/setup.sh first.")
    if os.path.exists(DB):
        os.remove(DB)
    con = sqlite3.connect(DB)
    cur = con.cursor()
    cols = ["name", "set_code", "set_name", "set_type", "released_at", "type_line",
            "mana_cost", "cmc", "oracle_text", "power", "toughness", "loyalty",
            "layout", "rarity", "arena_id", "oracle_id", "reprint", "digital",
            "color_identity", "colors", "keywords", "produced_mana", "card_faces"]
    cur.execute(f"CREATE TABLE cards ({', '.join(c + ' TEXT' for c in cols)});")
    placeholders = ", ".join("?" for _ in cols)
    ins = f"INSERT INTO cards ({', '.join(cols)}) VALUES ({placeholders});"

    batch, count = [], 0
    for c in rows(SRC):
        vals = [c.get(k) for k in SCALAR]
        vals += [json.dumps(c.get(k)) if c.get(k) is not None else None for k in JSONCOL]
        batch.append(vals)
        count += 1
        if len(batch) >= 5000:
            cur.executemany(ins, batch)
            batch.clear()
    if batch:
        cur.executemany(ins, batch)
    cur.execute("CREATE INDEX idx_name ON cards(name);")
    cur.execute("CREATE INDEX idx_name_nocase ON cards(name COLLATE NOCASE);")
    cur.execute("CREATE INDEX idx_oracle ON cards(oracle_id);")
    con.commit()
    con.close()
    print(f"    built {DB} ({count} printings, {os.path.getsize(DB) // (1024*1024)}MB).")


if __name__ == "__main__":
    main()
