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
    # Wall-clock span of the run, from the event timestamps already in the TB files — total runtime
    # (and overall steps/sec) for EVERY run, past ones included, with zero training-side overhead.
    wall_min, wall_max = math.inf, -math.inf
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
                if e.wall_time < wall_min:
                    wall_min = e.wall_time
                if e.wall_time > wall_max:
                    wall_max = e.wall_time
                v = float(e.value)
                if math.isfinite(v):  # NaN/inf points (e.g. block_double_rate with 0 blocks) are "no data"
                    dst[int(e.step)] = v  # later event files win on step collisions
    # winrate_vs_initial (beat-your-own-random-init) judged uninteresting by the user (2026-07-09)
    # — dropped from the dashboard for ALL runs, past ones included; trainers stop emitting it too.
    metrics = {t: pts for t, pts in metrics.items()
               if pts and not t.startswith("selfplay/winrate_vs_initial")}
    if not metrics:
        return None
    compact = {tag: _decimate([[s, round(v, 6)] for s, v in sorted(pts.items())])
               for tag, pts in metrics.items()}
    _atomic_write(os.path.join(out_dir, "runs", f"{name}.json"), {"name": name, "metrics": compact})
    max_step = max(pts[-1][0] for pts in compact.values())
    runtime_s = (wall_max - wall_min) if wall_max > wall_min else None
    return {
        "name": name,
        "experiment": group_for(name),
        "description": tb2aim.desired_description(name, path, notes),
        "tags": sorted(compact),
        "max_step": max_step,
        "points": sum(len(p) for p in compact.values()),
        "runtime_s": round(runtime_s, 1) if runtime_s else None,
        "sps": round(max_step / runtime_s, 1) if runtime_s and max_step else None,
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
    now = time.time()
    for e in entries:
        # A run whose event files were written in the last 3 minutes is training right now.
        e["live"] = (now - e.get("updated", 0)) < 180
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
    # VERSIONED layout (data/elo/<env>/meta.json + v<N>/{ratings,agents}.json): payload per env is
    # {current: N, versions: {N: <ratings.json + merged registry notes>}} — the shape the frontend
    # renders (defaults to current, older versions marked historical). Flat pre-versioning layout
    # still supported as a fallback.
    def _load_table(d):
        p = os.path.join(d, "ratings.json")
        if not os.path.isfile(p):
            return None
        with open(p) as f:
            table = json.load(f)
        reg_p = os.path.join(d, "agents.json")
        if os.path.isfile(reg_p):
            with open(reg_p) as f:
                reg = json.load(f)
            for name, a in table.get("agents", {}).items():
                a["notes"] = reg.get(name, {}).get("notes")
                a["kind"] = reg.get(name, {}).get("kind")
        return table

    elo = {}
    elo_root = os.path.join(REPO, "data", "elo")
    if os.path.isdir(elo_root):
        for env in sorted(os.listdir(elo_root)):
            env_dir = os.path.join(elo_root, env)
            try:
                meta_p = os.path.join(env_dir, "meta.json")
                if os.path.isfile(meta_p):
                    with open(meta_p) as f:
                        meta = json.load(f)
                    versions = {}
                    for v in meta.get("versions", {}):
                        t = _load_table(os.path.join(env_dir, f"v{v}"))
                        if t:
                            versions[str(v)] = t
                    if versions:
                        cur = str(meta.get("current"))
                        # A crosstable appended to in the last 3 minutes = tournament in progress.
                        gj = os.path.join(env_dir, f"v{cur}", "games.jsonl")
                        active = os.path.isfile(gj) and (time.time() - os.path.getmtime(gj)) < 180
                        # Tournament progress bar: game rows naming an agent that ISN'T in the
                        # current ratings table yet = a `rate_agent add` in flight (the table is
                        # only refit at the end). played = that agent's games so far; expected
                        # total = 100 games/seat × 2 seats vs every rated member. No backend
                        # cooperation needed, so it works for a tournament already running.
                        progress = None
                        if active:
                            try:
                                cur_agents = set(versions.get(cur, {}).get("agents", {}))
                                played = {}
                                with open(gj) as f:
                                    for line in f:
                                        try:
                                            g = json.loads(line)
                                        except ValueError:
                                            continue
                                        for nm in (g.get("a"), g.get("b")):
                                            if nm and nm not in cur_agents:
                                                played[nm] = played.get(nm, 0) + int(g.get("n", 0))
                                if played:
                                    nm, n = max(played.items(), key=lambda kv: kv[1])
                                    progress = {"agent": nm, "played": n,
                                                "total": max(len(cur_agents), 1) * 200,
                                                "updated": os.path.getmtime(gj)}
                            except Exception:
                                progress = None
                        elo[env] = {"current": cur, "versions": versions, "active": active,
                                    "progress": progress}
                else:  # legacy flat layout
                    t = _load_table(env_dir)
                    if t:
                        elo[env] = t
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
