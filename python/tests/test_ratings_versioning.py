"""Versioned rating environments (mtgenv_gym/evalkit/ratings.py).

A rating pool is only comparable within one engine contract, so pools are versioned: flat legacy data
migrates to v1 (idempotent), `bump-env` opens a clean v(N+1) with the spine re-seeded and an empty
crosstable, old versions are read-only, the contract fingerprint guards obs-shape/action-dim drift,
and ratings.json carries its env_version + fingerprint.
"""

import json

import pytest

from mtgenv_gym.evalkit.ratings import (
    ANCHOR_NAME,
    AgentEntry,
    GameRow,
    append_games,
    bump_version,
    current_version,
    ensure_initialized,
    fingerprints_compatible,
    list_versions,
    load_games,
    load_meta,
    load_registry,
    set_fingerprint,
    write_ratings,
)

_FP1 = {"obs_spec": {"bf_feat": [32, 48], "globals": [1, 69]}, "action_dim": 98, "engine_sha": "aaa"}
_FP2 = {"obs_spec": {"bf_feat": [32, 49], "globals": [1, 69]}, "action_dim": 98, "engine_sha": "bbb"}


def _row(a, b, aw, bw, seed=0):
    return GameRow(a=a, b=b, a_wins=aw, b_wins=bw, draws=0, n=aw + bw, seed=seed)


# ── migration ────────────────────────────────────────────────────────────────────────────────────
def test_migration_flat_to_v1_idempotent(tmp_path):
    env = "heralds"
    d = tmp_path / env
    d.mkdir(parents=True)
    (d / "agents.json").write_text(json.dumps({"random": {"kind": "random", "checkpoint": None, "notes": ""}}))
    (d / "games.jsonl").write_text(json.dumps({"a": "random", "b": "x", "a_wins": 1, "b_wins": 1,
                                               "draws": 0, "n": 2, "seed": 0, "mode": "greedy"}) + "\n")
    (d / "ratings.json").write_text("{}")

    v = ensure_initialized(env, root=tmp_path, fingerprint=_FP1)
    assert v == 1
    assert not (d / "agents.json").exists()             # flat files MOVED into v1
    assert (d / "v1" / "agents.json").exists()
    assert (d / "v1" / "games.jsonl").exists()
    meta = load_meta(env, root=tmp_path)
    assert meta["current"] == 1
    assert meta["versions"]["1"]["reason"] == "initial"
    assert meta["versions"]["1"]["fingerprint"] == _FP1

    # idempotent: a second init is a no-op — no new version, data intact
    assert ensure_initialized(env, root=tmp_path) == 1
    assert list(load_meta(env, root=tmp_path)["versions"]) == ["1"]
    rows = load_games(env, root=tmp_path)
    assert len(rows) == 1 and rows[0].a == "random"
    assert set(load_registry(env, root=tmp_path)) == {"random"}


def test_fresh_env_creates_empty_v1(tmp_path):
    v = ensure_initialized("swine", root=tmp_path, fingerprint=_FP1)
    assert v == 1
    assert load_games("swine", root=tmp_path) == []
    assert (tmp_path / "swine" / "v1").is_dir()


# ── bump ─────────────────────────────────────────────────────────────────────────────────────────
def test_bump_creates_clean_v2_retaining_history(tmp_path):
    env = "swine"
    append_games(env, [_row("random", "x", 1, 1)], root=tmp_path)   # inits v1
    set_fingerprint(env, 1, _FP1, root=tmp_path)
    reg = {"random": AgentEntry("random", "random", None, ""),
           "script-racer": AgentEntry("script-racer", "script_racer", None, "")}

    n = bump_version(env, reason="added a bf_feat column", fingerprint=_FP2, registry=reg, root=tmp_path)
    assert n == 2
    assert current_version(env, root=tmp_path) == 2
    assert load_games(env, root=tmp_path) == []                     # v2 crosstable is empty
    assert len(load_games(env, root=tmp_path, version=1)) == 1      # v1 history retained
    assert set(load_registry(env, root=tmp_path)) == {"random", "script-racer"}  # spine re-seeded

    vers = list_versions(env, root=tmp_path)
    assert set(vers) == {1, 2}
    assert vers[1]["fingerprint"] == _FP1
    assert vers[2]["fingerprint"] == _FP2
    assert vers[2]["reason"] == "added a bf_feat column"
    # v2 ratings.json exists and is stamped with the new version + fingerprint
    disk = json.loads((tmp_path / env / "v2" / "ratings.json").read_text())
    assert disk["env_version"] == 2 and disk["fingerprint"] == _FP2


def test_append_refuses_historical_version(tmp_path):
    env = "swine"
    append_games(env, [_row("random", "x", 1, 1)], root=tmp_path)
    bump_version(env, reason="x", fingerprint=_FP2, registry={}, root=tmp_path)  # current is now v2
    with pytest.raises(ValueError, match="only the current version"):
        append_games(env, [_row("random", "y", 1, 0, seed=9)], root=tmp_path, version=1)


def test_read_historical_version(tmp_path):
    env = "heralds"
    append_games(env, [_row("random", "x", 3, 1)], root=tmp_path)
    bump_version(env, reason="x", fingerprint=_FP2, registry={}, root=tmp_path)
    # refit an old version reads ITS crosstable, not the current empty one
    old = write_ratings(env, root=tmp_path, version=1)
    assert old["env_version"] == 1 and old["n_rows"] == 1
    assert write_ratings(env, root=tmp_path)["n_rows"] == 0   # current v2 is empty


# ── fingerprint guard ──────────────────────────────────────────────────────────────────────────────
def test_fingerprints_compatible_guard():
    same_but_sha = {**_FP1, "engine_sha": "zzz"}
    assert fingerprints_compatible(_FP1, same_but_sha)          # sha is provenance, not guarded
    assert not fingerprints_compatible(_FP1, _FP2)              # obs shape drift
    assert not fingerprints_compatible(_FP1, {**_FP1, "action_dim": 100})  # action dim drift
    assert fingerprints_compatible({}, _FP1)                    # empty stored ⇒ unknown ⇒ ok
    assert fingerprints_compatible(None, _FP1)


def test_ratings_carries_version_and_fingerprint(tmp_path):
    env = "heralds"
    append_games(env, [_row("random", "a", 2, 8), _row("a", "random", 9, 1, seed=5)], root=tmp_path)
    set_fingerprint(env, 1, _FP1, root=tmp_path)
    out = write_ratings(env, root=tmp_path)
    assert out["env_version"] == 1
    assert out["fingerprint"] == _FP1
    assert out["agents"]["a"]["rating"] > out["agents"][ANCHOR_NAME]["rating"]
