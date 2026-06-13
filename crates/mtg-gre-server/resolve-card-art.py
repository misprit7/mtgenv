import json, subprocess
# grp_id -> exact Scryfall name. MUST stay in sync with `mtg_core::cards::starter_db()` (every
# registered card). Checksum: starter_db().len() == len(cards) == 38 (see cards/mod.rs test).
# Sources: cards/misc/* (grp 1-65) + per-set folders (lea/dsk/fin/fdn/eoe/dft, grp 100-105).
cards = {
    # basics + prototype pool (cards/misc)
    1: "Plains", 2: "Island", 3: "Mountain", 4: "Forest",
    10: "Grizzly Bears", 11: "Hill Giant",
    20: "Shock", 21: "Divination", 23: "Lightning Bolt",
    30: "Elvish Visionary", 31: "Flametongue Kavu", 34: "Exultant Cultist",
    35: "Root Maze", 36: "Hardened Scales",
    40: "Glorious Anthem", 41: "Levitation", 43: "Nature's Revolt",
    # evergreen-keyword bodies + removal/equipment/aura (#14 breadth)
    50: "Elvish Archers", 51: "Fencing Ace", 52: "Argothian Swine", 53: "Typhoid Rats",
    54: "Child of Night", 55: "Alaborn Grenadier", 56: "Alley Strangler", 57: "Wall of Stone",
    58: "Murder", 59: "Darksteel Myr", 60: "Raging Goblin", 61: "King Cheetah",
    62: "Gladecover Scout", 64: "Bonesplitter", 65: "Pacifism",
    # real per-set cards (first-printing folders)
    100: "Llanowar Elves", 101: "Hushwood Verge", 102: "Sazh's Chocobo",
    103: "Mossborn Hydra", 104: "Icetill Explorer", 105: "Lumbering Worldwagon",
}
body = json.dumps({"identifiers":[{"name":n} for n in cards.values()]})
# one batch request to /cards/collection (≤75 ids per call)
out = subprocess.run(["curl","-s","-X","POST","https://api.scryfall.com/cards/collection",
    "-H","Content-Type: application/json","-H","Accept: application/json",
    "-H","User-Agent: mtgenv/0.1 (research)","-d",body], capture_output=True, text=True).stdout
resp = json.loads(out)
byname = {c["name"]: c for c in resp.get("data",[])}
manifest = {}
for gid,name in cards.items():
    c = byname.get(name)
    if not c:
        print("NOT FOUND:", name); continue
    iu = c.get("image_uris",{})
    manifest[str(gid)] = {"name":name, "art":iu.get("art_crop"), "img":iu.get("normal"), "artist":c.get("artist")}
json.dump(manifest, open("crates/mtg-gre-server/card-art.json","w"), indent=1, ensure_ascii=False)
print("wrote", len(manifest), "entries; not_found:", resp.get("not_found"))
