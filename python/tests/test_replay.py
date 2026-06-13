"""Validate the training-replay accessor end-to-end through Python — the `mtg_py.PyGame`
`record_replay` + `replay_json` path against the engine's locked `Replay` serde schema (a533720).
This is the shape webui's replay viewer also consumes, so we assert the JSON contract here.
"""

import json

import numpy as np
import mtg_py


def _play_to_end(game, seed=0):
    obs, mask, seat, request, num_legal, terminal = game.reset(seed)
    rng = np.random.default_rng(seed)
    while not terminal:
        legal = np.flatnonzero(np.asarray(mask, dtype=bool))
        game.apply(int(rng.choice(legal)))
        obs, mask, seat, request, num_legal, terminal = game.step_to_decision()


def test_replay_json_schema_roundtrip():
    g = mtg_py.PyGame("burn_vs_bears", auto_pass=True, record_replay=True, replay_step=4200)
    # No replay before the game finishes.
    assert g.replay_json(0) is None
    _play_to_end(g, seed=7)

    js = g.replay_json(1_718_000_000_000, names=["policy@4200", "random"], decks=["burn", "bears"])
    assert js is not None
    r = json.loads(js)

    # Metadata contract (caller-stamped fields + engine-filled result/source).
    assert r["meta"]["source"] == {"AiTraining": {"step": 4200}}
    assert r["meta"]["created_at"] == 1_718_000_000_000
    assert r["meta"]["result"] is not None, "engine fills result at game end"
    assert [p["name"] for p in r["meta"]["players"]] == ["policy@4200", "random"]
    assert [p["deck"] for p in r["meta"]["players"]] == ["burn", "bears"]

    # Frames: omniscient snapshots with labels; the first is the game-start frame.
    assert len(r["frames"]) > 1
    assert r["frames"][0]["label"] == "game start"
    assert "state" in r["frames"][0]


def test_no_replay_without_flag():
    g = mtg_py.PyGame("lands", auto_pass=True)  # record_replay defaults False
    _play_to_end(g, seed=1)
    assert g.replay_json(0) is None
