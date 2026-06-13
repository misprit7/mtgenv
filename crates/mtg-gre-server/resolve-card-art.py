import json, subprocess
# grp_id -> exact Scryfall name (our implemented starter set; extend as the pool grows)
cards = {1:"Plains",2:"Island",3:"Mountain",4:"Forest",10:"Grizzly Bears",11:"Hill Giant",
         20:"Shock",21:"Divination",22:"Healing Salve",23:"Lightning Bolt",30:"Elvish Visionary",
         31:"Flametongue Kavu",32:"Servant of the Scale",33:"Fog Bank"}
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
