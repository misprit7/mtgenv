"""Trainer launch guard — make the same-run double-launch collision *structurally* impossible.

Message passing between agents has no mutual exclusion, so a launch race (two `attn_train`/
`selfplay_train` starting the same run before either sees the other's message) can't be prevented by
protocol alone — it needs a lock at the process. At CLI startup, unless ``--force-launch``:
  (a) refuse if another attn_train/selfplay_train process is already running with the SAME --run-name
      (pgrep-based, exact token match);
  (b) refuse a --pool-dir that holds a LIVE pidfile (``RUNNING.pid``); a STALE pid (dead process) is
      treated as free and overwritten.
On acquire we write ``RUNNING.pid`` = our pid and register an atexit cleanup.

This is what would have prevented the 5.1 double-launch (dmc4 + lead both started 5.1-attn-v3-shape05
into /tmp/mtgenv_pool_5.1). The pidfile helpers are pure + unit-tested; the pgrep scan is best-effort.
"""
from __future__ import annotations

import atexit
import os
import subprocess
import sys

PIDFILE = "RUNNING.pid"


def _pid_alive(pid: int) -> bool:
    """True if a process with this pid exists (signal 0 probes without killing)."""
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True          # exists, owned by someone else
    except (ValueError, OverflowError):
        return False
    return True


def pidfile_path(pool_dir: str) -> str:
    return os.path.join(pool_dir, PIDFILE)


def live_pidfile(pool_dir: str) -> "int | None":
    """The pid in ``pool_dir/RUNNING.pid`` IF that process is still alive; None if absent or stale."""
    pf = pidfile_path(pool_dir)
    if not os.path.exists(pf):
        return None
    try:
        pid = int(open(pf).read().strip())
    except (ValueError, OSError):
        return None
    return pid if _pid_alive(pid) else None


def write_pidfile(pool_dir: str) -> None:
    """Write our pid to ``pool_dir/RUNNING.pid`` and remove it at process exit."""
    os.makedirs(pool_dir, exist_ok=True)
    pf = pidfile_path(pool_dir)
    with open(pf, "w") as f:
        f.write(str(os.getpid()))

    def _cleanup():
        try:
            if os.path.exists(pf) and int(open(pf).read().strip()) == os.getpid():
                os.remove(pf)
        except Exception:
            pass

    atexit.register(_cleanup)


def running_same_run_name(run_name: str) -> "list[int]":
    """Pids of OTHER attn_train/selfplay_train processes launched with this exact --run-name (pgrep;
    fail-soft to [] if pgrep is unavailable — the pidfile check is the hard lock)."""
    me = os.getpid()
    try:
        out = subprocess.run(["pgrep", "-af", r"attn_train\.py|selfplay_train\.py"],
                             capture_output=True, text=True, timeout=5).stdout
    except Exception:
        return []
    hits = []
    for line in out.splitlines():
        parts = line.split()
        if not parts or not parts[0].isdigit():
            continue
        pid = int(parts[0])
        if pid == me:
            continue
        toks = parts[1:]
        # exact token match: "--run-name <name>" or "--run-name=<name>"
        for i, t in enumerate(toks):
            if t == "--run-name" and i + 1 < len(toks) and toks[i + 1] == run_name:
                hits.append(pid)
            elif t == f"--run-name={run_name}":
                hits.append(pid)
    return hits


def acquire_launch(run_name: str, pool_dir: str, force: bool = False) -> None:
    """Guard a trainer launch. Exits the process (SystemExit) on a collision unless ``force``."""
    if not force:
        dup = running_same_run_name(run_name)
        if dup:
            sys.exit(f"[launch-guard] REFUSING: another trainer with --run-name {run_name!r} is already "
                     f"running (pid {dup}). Use --force-launch to override.")
        live = live_pidfile(pool_dir)
        if live is not None:
            sys.exit(f"[launch-guard] REFUSING: --pool-dir {pool_dir} has a live pidfile (pid {live}). "
                     f"Another run owns this pool. Use --force-launch to override.")
    write_pidfile(pool_dir)
