"""Bradley-Terry agent ratings (mtgenv_gym/evalkit/ratings.py).

Covers the math and the file layer without touching the engine: the penalized-MLE BT fit recovers a
known strength ordering and gaps, the anchor is pinned at exactly 1000, the crosstable round-trips
through JSONL, CIs behave (more games → tighter; separation → wide), and the nontransitivity report
flags a rock-paper-scissors triangle the single scalar cannot fit.
"""

import math

import numpy as np
import pytest

from mtgenv_gym.evalkit.ratings import (
    ANCHOR_NAME,
    ANCHOR_RATING,
    ELO_SCALE,
    GameRow,
    aggregate,
    append_games,
    compute_ratings,
    fit_bradley_terry,
    load_games,
    load_registry,
    register_agent,
    write_ratings,
)


def _synth_rows(theta_by_name, n=1000, seed_base=0):
    """Deterministic crosstable: every pair plays ``n`` games split to match the exact BT winrate."""
    names = list(theta_by_name)
    rows, s = [], seed_base
    for i in range(len(names)):
        for j in range(i + 1, len(names)):
            a, b = names[i], names[j]
            p = 1.0 / (1.0 + math.exp(-(theta_by_name[a] - theta_by_name[b])))
            a_wins = int(round(n * p))
            rows.append(GameRow(a=a, b=b, a_wins=a_wins, b_wins=n - a_wins, draws=0, n=n, seed=s))
            s += 1
    return rows


# ── the fit ────────────────────────────────────────────────────────────────────────────────────
def test_recovers_ordering_and_gaps():
    true = {ANCHOR_NAME: 0.0, "weak": -1.2, "mid": 0.6, "strong": 1.8}
    names, W, N = aggregate(_synth_rows(true, n=2000))
    theta, se, info = fit_bradley_terry(names, W, N)
    th = dict(zip(names, theta))

    # anchor pinned at 0 in theta space
    assert abs(th[ANCHOR_NAME]) < 1e-9
    # ordering recovered
    assert th["strong"] > th["mid"] > th[ANCHOR_NAME] > th["weak"]
    # gaps recovered (anchored, so absolute theta matches the true anchored values)
    for name, tv in true.items():
        assert abs(th[name] - tv) < 0.06, (name, th[name], tv)
    assert info["grad_norm"] < 1e-6


def test_anchor_is_exactly_1000(tmp_path):
    true = {ANCHOR_NAME: 0.0, "a": 0.5, "b": -0.5}
    append_games("heralds", _synth_rows(true, n=500), root=tmp_path)
    out = compute_ratings("heralds", root=tmp_path)
    assert out["agents"][ANCHOR_NAME]["rating"] == pytest.approx(ANCHOR_RATING, abs=1e-6)
    assert out["agents"][ANCHOR_NAME]["ci_lo"] == pytest.approx(ANCHOR_RATING, abs=1e-6)
    assert out["agents"][ANCHOR_NAME]["ci_hi"] == pytest.approx(ANCHOR_RATING, abs=1e-6)
    # elo mapping is consistent with the fitted theta of a non-anchor agent
    a = out["agents"]["a"]
    assert a["rating"] == pytest.approx(ANCHOR_RATING + ELO_SCALE * a["theta"], abs=0.1)
    assert a["rating"] > out["agents"]["b"]["rating"]  # theta 0.5 beats -0.5


# ── the file layer ───────────────────────────────────────────────────────────────────────────────
def test_crosstable_round_trips(tmp_path):
    rows = [
        GameRow(a="x", b="random", a_wins=7, b_wins=3, draws=0, n=10, seed=1, mode="greedy"),
        GameRow(a="random", b="x", a_wins=4, b_wins=5, draws=1, n=10, seed=2, mode="greedy"),
    ]
    append_games("swine", rows, root=tmp_path)
    back = load_games("swine", root=tmp_path)
    assert [r.__dict__ for r in back] == [r.__dict__ for r in rows]

    names, W, N = aggregate(back)
    xi, ri = names.index("x"), names.index("random")
    # x: 7 wins over random (row1) + 5 wins as b over random (row2) + 0.5 draw = 12.5
    assert W[xi, ri] == pytest.approx(7 + 5 + 0.5)
    assert W[ri, xi] == pytest.approx(3 + 4 + 0.5)
    assert N[xi, ri] == pytest.approx(20)  # 10 + 10 games between them, symmetric
    assert N[ri, xi] == pytest.approx(20)


def test_registry_round_trips(tmp_path):
    register_agent("swine", ANCHOR_NAME, "random", None, "the 1000 anchor", root=tmp_path)
    register_agent("swine", "ppo-final", "ppo", "/tmp/x.zip", "4.6 final", root=tmp_path)
    reg = load_registry("swine", root=tmp_path)
    assert set(reg) == {ANCHOR_NAME, "ppo-final"}
    assert reg["ppo-final"].kind == "ppo"
    assert reg["ppo-final"].checkpoint == "/tmp/x.zip"
    with pytest.raises(ValueError):
        register_agent("swine", "ppo-final", "ppo", root=tmp_path)  # dup without overwrite
    register_agent("swine", "ppo-final", "ppo", "/tmp/y.zip", root=tmp_path, overwrite=True)
    assert load_registry("swine", root=tmp_path)["ppo-final"].checkpoint == "/tmp/y.zip"


def test_registered_but_unplayed_agent_included(tmp_path):
    append_games("heralds", _synth_rows({ANCHOR_NAME: 0.0, "a": 0.5}, n=400), root=tmp_path)
    register_agent("heralds", "ghost", "ppo", "/tmp/none.zip", "never played", root=tmp_path)
    out = compute_ratings("heralds", root=tmp_path)
    assert "ghost" in out["agents"]
    assert out["agents"]["ghost"]["games"] == 0
    # no data ⇒ pinned near the anchor but with a very wide CI
    assert out["agents"]["ghost"]["ci_hi"] - out["agents"]["ghost"]["ci_lo"] > 500


# ── confidence intervals ─────────────────────────────────────────────────────────────────────────
def test_ci_tightens_with_more_games():
    # 'heavy' plays 4000 games total; 'light' plays 40 — both ~coin-flip vs anchor (not separated).
    rows = []
    for k in range(4000):
        w = 1 if (k % 2 == 0) else 0
        rows.append(GameRow(a="heavy", b=ANCHOR_NAME, a_wins=w, b_wins=1 - w, draws=0, n=1, seed=k))
    for k in range(40):
        w = 1 if (k % 2 == 0) else 0
        rows.append(GameRow(a="light", b=ANCHOR_NAME, a_wins=w, b_wins=1 - w, draws=0, n=1, seed=k))
    names, W, N = aggregate(rows)
    _, se, _ = fit_bradley_terry(names, W, N)
    se = dict(zip(names, se))
    assert se["heavy"] < se["light"]
    assert se["heavy"] > 0  # finite, positive


def test_separation_gives_finite_wide_ci(tmp_path):
    # 'perfect' wins every game vs random: MLE theta would be +inf; ridge keeps it finite + very wide.
    rows = [GameRow(a="perfect", b=ANCHOR_NAME, a_wins=200, b_wins=0, draws=0, n=200, seed=0)]
    append_games("swine", rows, root=tmp_path)
    out = compute_ratings("swine", root=tmp_path)
    p = out["agents"]["perfect"]
    assert p["rating"] > ANCHOR_RATING          # clearly stronger
    assert math.isfinite(p["rating"])           # but finite (ridge)
    assert p["ci_hi"] - p["ci_lo"] > 200        # wide — separation ⇒ low confidence


# ── nontransitivity ──────────────────────────────────────────────────────────────────────────────
def test_nontransitivity_flags_rock_paper_scissors():
    # A>B, B>C, C>A each 100-0 (plus everyone ~even vs the anchor). No transitive theta fits this.
    n = 100
    rows = [
        GameRow(a="A", b="B", a_wins=n, b_wins=0, draws=0, n=n, seed=1),
        GameRow(a="B", b="C", a_wins=n, b_wins=0, draws=0, n=n, seed=2),
        GameRow(a="C", b="A", a_wins=n, b_wins=0, draws=0, n=n, seed=3),
        GameRow(a="A", b=ANCHOR_NAME, a_wins=n // 2, b_wins=n // 2, draws=0, n=n, seed=4),
        GameRow(a="B", b=ANCHOR_NAME, a_wins=n // 2, b_wins=n // 2, draws=0, n=n, seed=5),
        GameRow(a="C", b=ANCHOR_NAME, a_wins=n // 2, b_wins=n // 2, draws=0, n=n, seed=6),
    ]
    names, W, N = aggregate(rows)
    theta, _, _ = fit_bradley_terry(names, W, N)
    from mtgenv_gym.evalkit.ratings import nontransitivity

    nt = nontransitivity(names, W, N, theta)
    # a 100-0 edge the scalar predicts as ~50/50 ⇒ residual near ±0.5
    assert abs(nt["max_residual"]) > 0.4


def test_write_ratings_persists_json(tmp_path):
    append_games("heralds", _synth_rows({ANCHOR_NAME: 0.0, "a": 1.0}, n=300), root=tmp_path)
    out = write_ratings("heralds", root=tmp_path)
    import json

    disk = json.loads((tmp_path / "heralds" / "v1" / "ratings.json").read_text())
    assert disk == out
    assert disk["anchor"] == ANCHOR_NAME
    assert disk["env_version"] == 1
    assert "a" in disk["agents"]
