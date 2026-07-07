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
import sys
import threading
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tb2aim  # event_dirs / notes_text / desired_description / experiment_for

from tensorboard.backend.event_processing.event_accumulator import EventAccumulator

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
STATIC_DIR = os.path.join(REPO, "python", "mtgenv_gym", "evalkit", "dashboard")
POINT_CAP = 2500  # per-series decimation cap (display resolution, not storage)


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
        "experiment": tb2aim.experiment_for(name),
        "description": tb2aim.desired_description(name, path, notes),
        "tags": sorted(compact),
        "max_step": max_step,
        "points": sum(len(p) for p in compact.values()),
    }


def sweep(tb_root: str, out_dir: str, cache: dict) -> None:
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
    _atomic_write(os.path.join(out_dir, "index.json"),
                  {"generated_at": time.time(), "runs": entries})


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
    ap.add_argument("--tb-dir", default="/tmp/mtgenv_tb")
    ap.add_argument("--out", default=os.path.join(REPO, "data", "dash"))
    ap.add_argument("--port", type=int, default=8060)
    ap.add_argument("--host", default="0.0.0.0")
    ap.add_argument("--interval", type=int, default=90)
    ap.add_argument("--once", action="store_true", help="one sync, no server")
    args = ap.parse_args()

    cache: dict = {}
    sweep(args.tb_dir, args.out, cache)
    if args.once:
        return

    def loop():
        while True:
            time.sleep(args.interval)
            try:
                sweep(args.tb_dir, args.out, cache)
            except Exception as e:
                print(f"[evaldash] sweep error: {e}", file=sys.stderr)

    threading.Thread(target=loop, daemon=True).start()
    Handler.data_dir = os.path.abspath(args.out)
    srv = ThreadingHTTPServer((args.host, args.port), Handler)
    print(f"[evaldash] serving {STATIC_DIR} + {args.out} on http://{args.host}:{args.port}")
    srv.serve_forever()


if __name__ == "__main__":
    main()
