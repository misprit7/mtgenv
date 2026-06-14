#!/usr/bin/env python3
"""Regenerate card-art.json — the baked-in Scryfall art manifest (grp_id -> art_crop/normal/artist).

The card list is NOT hand-maintained: it comes from the engine itself via the `dump-cards` binary
(every card in a selectable deck = every card a player can see). So whenever a card is added to a
deck, re-running this picks it up automatically — there's nothing to keep in sync by hand.

Usage:  python3 crates/mtg-gre-server/resolve-card-art.py
        (run from anywhere; needs network access to api.scryfall.com)
Then rebuild so the new manifest is baked in:  cargo build -p mtg-gre-server
"""
import json
import os
import subprocess
import sys

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
OUT_PATH = os.path.join(REPO_ROOT, "crates", "mtg-gre-server", "card-art.json")


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
        byname.update({c["name"]: c for c in resp.get("data", [])})
        if resp.get("not_found"):
            print("  not found:", [nf.get("name") for nf in resp["not_found"]])
    return byname


def main():
    cards = load_card_pool()
    byname = fetch_scryfall(list(cards.values()))
    manifest, missing = {}, []
    for gid, name in sorted(cards.items()):
        c = byname.get(name)
        if not c:
            missing.append(name)
            continue
        # Double-faced cards have no top-level image_uris; fall back to the front face.
        iu = c.get("image_uris") or (c.get("card_faces", [{}])[0].get("image_uris", {}))
        manifest[str(gid)] = {
            "name": name,
            "art": iu.get("art_crop"),
            "img": iu.get("normal"),
            "artist": c.get("artist"),
        }
    json.dump(manifest, open(OUT_PATH, "w"), indent=1, ensure_ascii=False)
    print(f"wrote {len(manifest)} entries to {os.path.relpath(OUT_PATH, REPO_ROOT)}")
    if missing:
        print("MISSING (no Scryfall match — check the exact card name):", missing)


if __name__ == "__main__":
    main()
