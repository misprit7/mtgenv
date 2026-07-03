"""M3 acceptance: engine behavioral-equivalence snapshot (gym side).

The engine is deterministic by seed, so a fixed (deck, seed, scripted-policy) suite must produce an
identical decision trajectory across transports. This pins the CURRENT transport's fingerprints in a
committed snapshot; when M3's fleet/Session path lands, re-run `fingerprint_suite(make_driver=...)`
with the new driver and `diff_suites` against this snapshot — any divergence is behavior drift.

Regenerate the snapshot ONLY after an intentional engine change (and eyeball the diff first):
    UPDATE_SNAPSHOT=1 PYTHONPATH=python python/.venv/bin/python -m pytest python/tests/test_equivalence.py
"""

import json
import os

import pytest

pytest.importorskip("numpy")
if pytest.importorskip("mtg_py", reason="mtg_py extension not built") is None:  # pragma: no cover
    pytest.skip("mtg_py not built", allow_module_level=True)

from mtgenv_gym.equivalence import diff_suites, fingerprint_suite  # noqa: E402

SNAPSHOT = os.path.join(os.path.dirname(__file__), "equivalence_fingerprints.json")


def test_engine_equivalence_snapshot():
    """Current transport reproduces the committed decision-trajectory fingerprints byte-for-byte."""
    current = fingerprint_suite()
    if os.environ.get("UPDATE_SNAPSHOT"):
        with open(SNAPSHOT, "w") as f:
            json.dump(current, f, indent=2)
        pytest.skip(f"snapshot regenerated ({len(current)} games) → {SNAPSHOT}")
    with open(SNAPSHOT) as f:
        expected = json.load(f)
    diffs = diff_suites(expected, current)
    assert not diffs, "engine behavior drifted from the committed snapshot:\n  " + "\n  ".join(diffs)


def test_engine_is_deterministic():
    """The property the equivalence test rests on: two runs of the suite are byte-identical."""
    assert diff_suites(fingerprint_suite(), fingerprint_suite()) == []


def test_fleet_transport_matches_snapshot():
    """M3.4 gate: the batched Fleet stepper (via a 1-env `_FleetDriver`) reproduces the committed
    decision-trajectory snapshot byte-for-byte — same behavior as the PyGame transport."""
    from mtgenv_gym.equivalence import fleet_driver

    with open(SNAPSHOT) as f:
        expected = json.load(f)
    diffs = diff_suites(expected, fingerprint_suite(make_driver=fleet_driver))
    assert not diffs, "Fleet transport diverged from the committed snapshot:\n  " + "\n  ".join(diffs)


def test_fleet_multiworker_is_deterministic():
    """M3.4 determinism gate: a 6-env / 3-worker Fleet's per-env trajectories each match the PyGame
    single-game reference for that seed — parallel stepping across worker threads doesn't perturb
    per-env behavior (independent Sessions; deterministic per-env seeding + assembly ordering)."""
    from mtgenv_gym.equivalence import fleet_multienv_fingerprints, game_fingerprint, pygame_driver

    n = 6
    fleet = fleet_multienv_fingerprints("bears", n, num_workers=3)
    ref = [game_fingerprint("bears", i, pygame_driver) for i in range(n)]
    diffs = diff_suites(ref, fleet)
    assert not diffs, "Fleet multi-worker diverged from PyGame per-env:\n  " + "\n  ".join(diffs)
