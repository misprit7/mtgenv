"""Launch-guard pidfile logic — the structural lock that makes a same-pool double-launch impossible.

Covers the pure/pidfile paths (write/alive/stale/acquire/force). The pgrep scan (running_same_run_name)
is best-effort and not unit-tested — the pidfile is the hard lock and what these tests pin down.
"""
import os
import subprocess
import sys
import tempfile

import pytest

from mtgenv_gym import launch_guard as lg


def _dead_pid():
    """A pid that is guaranteed dead: spawn a trivial process, reap it, return its (now-free) pid.
    Reuse within the microseconds before the assert is astronomically unlikely."""
    p = subprocess.Popen([sys.executable, "-c", "pass"])
    p.wait()
    return p.pid


def test_pid_alive():
    assert lg._pid_alive(os.getpid()) is True
    assert lg._pid_alive(_dead_pid()) is False
    assert lg._pid_alive(2**31 - 1) is False        # above pid_max → ESRCH


def test_live_pidfile_absent_is_none():
    d = tempfile.mkdtemp()
    assert lg.live_pidfile(d) is None               # no RUNNING.pid → free


def test_write_then_live_pidfile_reports_self():
    d = tempfile.mkdtemp()
    lg.write_pidfile(d)
    assert os.path.exists(lg.pidfile_path(d))
    assert lg.live_pidfile(d) == os.getpid()         # our own live pid


def test_stale_pidfile_reads_as_free():
    d = tempfile.mkdtemp()
    with open(lg.pidfile_path(d), "w") as f:
        f.write(str(_dead_pid()))
    assert lg.live_pidfile(d) is None                # dead pid → treated as free


def test_garbage_pidfile_reads_as_free():
    d = tempfile.mkdtemp()
    with open(lg.pidfile_path(d), "w") as f:
        f.write("not-a-pid")
    assert lg.live_pidfile(d) is None


def test_acquire_fresh_pool_writes_pidfile():
    d = tempfile.mkdtemp()
    lg.acquire_launch("some-run", d, force=False)
    assert lg.live_pidfile(d) == os.getpid()


def test_acquire_refuses_live_pool_pidfile():
    d = tempfile.mkdtemp()
    with open(lg.pidfile_path(d), "w") as f:
        f.write(str(os.getpid()))                    # a live pid owns this pool
    with pytest.raises(SystemExit):
        lg.acquire_launch("some-run", d, force=False)


def test_force_launch_overrides_live_pidfile():
    d = tempfile.mkdtemp()
    with open(lg.pidfile_path(d), "w") as f:
        f.write(str(os.getpid()))
    lg.acquire_launch("some-run", d, force=True)     # no raise — overwrites
    assert lg.live_pidfile(d) == os.getpid()


def test_acquire_over_stale_pidfile_succeeds():
    d = tempfile.mkdtemp()
    with open(lg.pidfile_path(d), "w") as f:
        f.write(str(_dead_pid()))                     # stale owner
    lg.acquire_launch("some-run", d, force=False)    # stale → free → acquire
    assert lg.live_pidfile(d) == os.getpid()
