#!/usr/bin/env python3
"""evaldash — the mtgenv training dashboard (mobile-first, self-hosted).

One daemon that (a) periodically syncs every TB run dir under ``--tb-dir`` into compact JSON at
``data/dash/`` and (b) serves the static dashboard page (``python/mtgenv_gym/evalkit/dashboard/``)
plus that JSON. TB event files remain the write format (same contract as scripts/tb2aim.py, whose
readers + description logic this reuses).

    experiments/muzero2/.venv/bin/python scripts/evaldash.py            # sync every 90s + serve :8060
    experiments/muzero2/.venv/bin/python scripts/evaldash.py --once     # one sync, no server

Data layout (regenerated in place, atomic renames):
    data/dash/index.json        {generated_at, runs: [{name, experiment, description, tags,
                                 max_step, points, updated}]}
    data/dash/runs/<name>.json  {name, metrics: {tag: [[step, value], ...]}}   (decimated)
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import sys
import threading
import time
import urllib.request
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tb2aim  # event_dirs / notes_text / desired_description

from tensorboard.backend.event_processing.event_accumulator import EventAccumulator

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
STATIC_DIR = os.path.join(REPO, "python", "mtgenv_gym", "evalkit", "dashboard")
POINT_CAP = 2500  # per-series decimation cap (display resolution, not storage)


def group_for(name: str) -> str:
    """Sections = MAJOR VERSIONS (user preference: numbers carry chronology). Non-versioned runs
    split into 'reference' (yardstick measurements) and 'misc' (smokes, superseded one-offs)."""
    m = re.match(r"^(\d+)\.", name)
    if m:
        return f"{m.group(1)}.x"
    if name.startswith("ref"):
        return "reference"
    return "misc"


def fetch_replays(lobby: str) -> "list[dict]":
    """All replay metas from the game server's /api/replays, oldest→newest (fail-soft: empty list
    when the lobby is down). Matched to runs in sweep(), where the run names are known."""
    try:
        with urllib.request.urlopen(f"{lobby}/api/replays", timeout=3) as r:
            return sorted(json.load(r), key=lambda x: x.get("created_at", 0))
    except Exception:
        return []


def _atomic_write(path: str, obj) -> None:
    tmp = path + ".tmp"
    with open(tmp, "w") as f:
        # allow_nan=False: bare NaN is invalid JSON — JS JSON.parse rejects it (python accepts it,
        # which hides the bug). Non-finite points are filtered at collection; this backstops.
        json.dump(obj, f, separators=(",", ":"), allow_nan=False)
    os.replace(tmp, path)


def _decimate(series: "list[list]") -> "list[list]":
    if len(series) <= POINT_CAP:
        return series
    stride = -(-len(series) // POINT_CAP)
    out = series[::stride]
    if out[-1] != series[-1]:
        out.append(series[-1])
    return out


def _run_mtime(path: str) -> float:
    """Newest mtime across the run's event files + description.md — cheap change detection."""
    newest = 0.0
    for d in tb2aim.event_dirs(path):
        for f in os.listdir(d):
            if f.startswith("events.out.tfevents"):
                try:
                    newest = max(newest, os.path.getmtime(os.path.join(d, f)))
                except OSError:
                    pass
    desc = os.path.join(path, "description.md")
    if os.path.isfile(desc):
        newest = max(newest, os.path.getmtime(desc))
    return newest


def sync_run(tb_root: str, name: str, out_dir: str) -> "dict | None":
    """Regenerate runs/<name>.json; returns the index entry (None if the run has no scalars)."""
    path = os.path.join(tb_root, name)
    metrics: "dict[str, dict[int, float]]" = {}
    notes = None
    for d in tb2aim.event_dirs(path):
        acc = EventAccumulator(d, size_guidance={"scalars": 0})
        try:
            acc.Reload()
        except Exception as e:
            print(f"[evaldash] WARN {name}: {e}", file=sys.stderr)
            continue
        notes = notes or tb2aim.notes_text(acc)
        for tag in acc.Tags().get("scalars", []):
            dst = metrics.setdefault(tag, {})
            for e in acc.Scalars(tag):
                v = float(e.value)
                if math.isfinite(v):  # NaN/inf points (e.g. block_double_rate with 0 blocks) are "no data"
                    dst[int(e.step)] = v  # later event files win on step collisions
    metrics = {t: pts for t, pts in metrics.items() if pts}
    if not metrics:
        return None
    compact = {tag: _decimate([[s, round(v, 6)] for s, v in sorted(pts.items())])
               for tag, pts in metrics.items()}
    _atomic_write(os.path.join(out_dir, "runs", f"{name}.json"), {"name": name, "metrics": compact})
    max_step = max(pts[-1][0] for pts in compact.values())
    return {
        "name": name,
        "experiment": group_for(name),
        "description": tb2aim.desired_description(name, path, notes),
        "tags": sorted(compact),
        "max_step": max_step,
        "points": sum(len(p) for p in compact.values()),
    }


def sweep(tb_root: str, out_dir: str, cache: dict, lobby: str) -> None:
    os.makedirs(os.path.join(out_dir, "runs"), exist_ok=True)
    entries = []
    for name in sorted(os.listdir(tb_root)):
        path = os.path.join(tb_root, name)
        if not os.path.isdir(path):
            continue
        mt = _run_mtime(path)
        cached = cache.get(name)
        if cached and cached["mtime"] >= mt:
            entries.append(cached["entry"])
            continue
        entry = sync_run(tb_root, name, out_dir)
        if entry:
            entry["updated"] = mt
            cache[name] = {"mtime": mt, "entry": entry}
            entries.append(entry)
            print(f"[evaldash] synced {name} ({entry['points']} pts)")
    replays = fetch_replays(lobby)  # oldest→newest, so the last match per run is the latest
    for e in entries:
        # SB3 appends _N to the TB dir name ("4.4-ppo-heralds_1") but replays are recorded under
        # the bare run name — match both.
        base = re.sub(r"_\d+$", "", e["name"])
        prefixes = {f"aitrain-{e['name']}-", f"aitrain-{base}-"}
        mine = [r for r in replays if any(r.get("id", "").startswith(p) for p in prefixes)]
        e["replays"] = len(mine)
        e["latest_replay"] = mine[-1]["id"] if mine else None
        # Full list for the dashboard's replays panel: id + training step (parsed from the id) + time.
        e["replay_list"] = [
            {"id": r["id"],
             "step": int(m.group(1)) if (m := re.search(r"-step(\d+)-", r["id"])) else None,
             "t": r.get("created_at", 0)}
            for r in mine
        ]
    # Elo/BT rating tables (written by python/rate_agent.py) — fail-soft when absent.
    elo = {}
    elo_root = os.path.join(REPO, "data", "elo")
    if os.path.isdir(elo_root):
        for env in sorted(os.listdir(elo_root)):
            p = os.path.join(elo_root, env, "ratings.json")
            if os.path.isfile(p):
                try:
                    with open(p) as f:
                        elo[env] = json.load(f)
                except Exception:
                    pass
    _atomic_write(os.path.join(out_dir, "index.json"),
                  {"generated_at": time.time(), "runs": entries, "elo": elo})


class Handler(SimpleHTTPRequestHandler):
    """Static dashboard from STATIC_DIR; /data/* from the JSON out dir; everything no-cache
    except the vendored chart lib (content-addressed enough by version bumps)."""

    data_dir = ""  # set in main()

    def translate_path(self, path):
        path = path.split("?", 1)[0].split("#", 1)[0]
        if path.startswith("/data/"):
            return os.path.join(self.data_dir, os.path.normpath(path[len("/data/"):]))
        if path in ("", "/"):
            path = "/index.html"
        return os.path.join(STATIC_DIR, os.path.normpath(path.lstrip("/")))

    def end_headers(self):
        if not self.path.endswith(".js"):
            self.send_header("Cache-Control", "no-cache")
        super().end_headers()

    def log_message(self, fmt, *args):  # quiet
        pass


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--tb-dir", default="/home/xander/dev/p-mtg/mtgenv/data/tb")
    ap.add_argument("--out", default=os.path.join(REPO, "data", "dash"))
    ap.add_argument("--port", type=int, default=8060)
    ap.add_argument("--host", default="0.0.0.0")
    ap.add_argument("--interval", type=int, default=90)
    ap.add_argument("--lobby", default="http://127.0.0.1:8080",
                    help="game server base URL for replay counts (fail-soft when down)")
    ap.add_argument("--once", action="store_true", help="one sync, no server")
    args = ap.parse_args()

    cache: dict = {}
    sweep(args.tb_dir, args.out, cache, args.lobby)
    if args.once:
        return

    def loop():
        while True:
            time.sleep(args.interval)
            try:
                sweep(args.tb_dir, args.out, cache, args.lobby)
            except Exception as e:
                print(f"[evaldash] sweep error: {e}", file=sys.stderr)

    threading.Thread(target=loop, daemon=True).start()
    Handler.data_dir = os.path.abspath(args.out)
    srv = ThreadingHTTPServer((args.host, args.port), Handler)
    print(f"[evaldash] serving {STATIC_DIR} + {args.out} on http://{args.host}:{args.port}")
    srv.serve_forever()


if __name__ == "__main__":
    main()
