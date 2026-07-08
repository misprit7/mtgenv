"""Bradley-Terry agent ratings — the model-quality scalar for hill climbing.

Per-environment rating pools, **versioned by the engine contract**. A rating is only meaningful within
one (obs-space × action-space × engine) contract — any change there invalidates comparability, so each
contract gets its own version with its own crosstable; history is retained and the *current* version is
the default view. Layout under ``data/elo/<env>/`` (gitignored via ``/data/``):

    meta.json                     {"current": N, "versions": {"N": {created, reason, fingerprint}}}
    v<N>/agents.json              registry: {name: {kind, checkpoint, notes}}
    v<N>/games.jsonl              append-only crosstable, one row per Arena batch (one seating)
    v<N>/ratings.json             fitted output (also carries env_version + fingerprint)

The **fingerprint** captures the contract: per-key obs shapes (``mtg_py.PyGame.obs_spec()``), the
``action_dim``, and the engine git sha at creation. Game-playing commands compare the live engine's
fingerprint against the current version's stored one and REFUSE on a mismatch (obs shapes or action
dim) — the mechanism that makes "we changed the action/obs space" impossible to forget; the operator
runs ``bump-env`` to open a fresh version. Everything below is numpy-only and never imports the engine
— the *live* fingerprint is computed by the caller (see ``rate_agent.engine_fingerprint``) and passed
in as a plain dict; this module only stores/compares them.

The model is Bradley-Terry: agent *i* has strength ``theta_i`` and ``P(i beats j) = sigmoid(theta_i -
theta_j)``. We fit ``theta`` by penalized maximum likelihood (a small logistic regression) over the
AGGREGATED crosstable of one version, pin ``random`` so the scale is identified, and map to an Elo-like
scale ``rating = 1000 + (400/ln 10)·theta``. Draws count as half a win to each side. CIs come from the
Fisher information; a tiny ridge keeps ``theta`` finite under separation.
"""

from __future__ import annotations

import datetime
import json
import math
import os
import tempfile
from dataclasses import asdict, dataclass
from pathlib import Path

import numpy as np

# Elo scale: a one-logit (theta) strength gap == 400/ln10 ≈ 173.72 Elo, so
# P(win) = 1/(1 + 10^(-ΔElo/400)) = sigmoid(Δtheta) — the BT logit and Elo agree.
ELO_SCALE = 400.0 / math.log(10.0)
ANCHOR_NAME = "random"
ANCHOR_RATING = 1000.0

# Environments with a v1 rating pool (both mirror decks). SOS/random-deck envs are deferred.
ENVS = ("heralds", "swine")

_FLAT_FILES = ("agents.json", "games.jsonl", "ratings.json")   # legacy pre-versioning layout


# ── paths ──────────────────────────────────────────────────────────────────────────────────────────
def elo_root(root: "str | os.PathLike | None" = None) -> Path:
    """Resolve the ``data/elo`` root. Override with ``root=`` (tests) or ``$MTGENV_ELO_ROOT``."""
    if root is not None:
        return Path(root)
    env = os.environ.get("MTGENV_ELO_ROOT")
    if env:
        return Path(env)
    # ratings.py -> evalkit -> mtgenv_gym -> python -> <repo root>
    return Path(__file__).resolve().parents[3] / "data" / "elo"


def env_root(env: str, root=None) -> Path:
    return elo_root(root) / env


def meta_path(env: str, root=None) -> Path:
    return env_root(env, root) / "meta.json"


def version_dir(env: str, version: int, root=None) -> Path:
    return env_root(env, root) / f"v{int(version)}"


def _atomic_write(path: Path, text: str) -> None:
    """Write ``text`` to ``path`` via a temp file + ``os.replace`` (atomic; no torn reads)."""
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=str(path.parent), suffix=".tmp")
    try:
        with os.fdopen(fd, "w") as f:
            f.write(text)
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.remove(tmp)


def _now_iso() -> str:
    return datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")


# ── meta.json (the version index) ────────────────────────────────────────────────────────────────
def load_meta(env: str, root=None) -> "dict | None":
    p = meta_path(env, root)
    return json.loads(p.read_text()) if p.exists() else None


def save_meta(env: str, meta: dict, root=None) -> None:
    _atomic_write(meta_path(env, root), json.dumps(meta, indent=2, sort_keys=True) + "\n")


def current_version(env: str, root=None) -> "int | None":
    m = load_meta(env, root)
    return int(m["current"]) if m else None


def list_versions(env: str, root=None) -> "dict[int, dict]":
    m = load_meta(env, root) or {"versions": {}}
    return {int(k): v for k, v in m.get("versions", {}).items()}


def ensure_initialized(env: str, root=None, *, reason: str = "initial",
                       fingerprint: "dict | None" = None) -> int:
    """Guarantee ``meta.json`` + a current version dir exist; return the current version.

    Idempotent and race-safe: if ``meta.json`` is present we're done. Otherwise MIGRATE any legacy
    flat files (``<env>/agents.json`` …) into ``v1`` (atomic per-file move) and write ``meta`` — the
    single atomic ``meta`` write is the commit point, so a concurrent writer either sees no meta (and
    funnels through this same migration) or the finished versioned layout, never a split brain. A fresh
    env with no data just gets an empty ``v1``. ``fingerprint`` records the contract for the created
    version (callers with the engine pass the live one; ``None`` → ``{}``, backfilled later)."""
    if load_meta(env, root) is not None:
        return current_version(env, root)
    v1 = version_dir(env, 1, root)
    v1.mkdir(parents=True, exist_ok=True)
    er = env_root(env, root)
    for fn in _FLAT_FILES:                       # migrate legacy flat data (move; skip if already gone)
        src, dst = er / fn, v1 / fn
        if src.exists() and not dst.exists():
            os.replace(str(src), str(dst))
    if load_meta(env, root) is None:             # re-check right before the commit (lost-race guard)
        save_meta(env, {"current": 1, "versions": {
            "1": {"created": _now_iso(), "reason": reason, "fingerprint": fingerprint or {}}}}, root)
    return current_version(env, root)


def set_fingerprint(env: str, version: int, fingerprint: dict, root=None) -> None:
    """Record (or backfill) the contract fingerprint of ``version`` in ``meta.json``."""
    m = load_meta(env, root)
    if m is None or str(version) not in m["versions"]:
        raise ValueError(f"{env} v{version} is not initialized")
    m["versions"][str(version)]["fingerprint"] = fingerprint
    save_meta(env, m, root)


def bump_version(env: str, *, reason: str, fingerprint: dict, registry=None, root=None) -> int:
    """Open a fresh version v(N+1): (optionally spine-seeded) registry, EMPTY crosstable, recorded
    fingerprint; set it current. Returns the new version. Trained agents re-join via ``register`` +
    tournaments. History (older versions + their games) is retained untouched."""
    ensure_initialized(env, root, fingerprint=fingerprint, reason=reason)
    m = load_meta(env, root)
    n = max(int(k) for k in m["versions"]) + 1
    vd = version_dir(env, n, root)
    vd.mkdir(parents=True, exist_ok=True)
    (vd / "games.jsonl").write_text("")                       # empty crosstable
    reg = registry or {}
    _atomic_write(vd / "agents.json", json.dumps(
        {name: {k: v for k, v in asdict(e).items() if k != "name"} for name, e in reg.items()},
        indent=2, sort_keys=True) + "\n")
    m["versions"][str(n)] = {"created": _now_iso(), "reason": reason, "fingerprint": fingerprint}
    m["current"] = n
    save_meta(env, m, root)
    write_ratings(env, root=root, version=n)
    return n


def resolve_version(env: str, version: "int | None" = None, root=None) -> int:
    """The version an op targets: an explicit ``version`` (must already exist; for read-only history)
    or the current version (initializing/migrating the env if needed)."""
    if version is not None:
        if not version_dir(env, version, root).exists():
            raise ValueError(f"{env} v{version} does not exist "
                             f"(versions: {sorted(list_versions(env, root))})")
        return int(version)
    return ensure_initialized(env, root)


def active_dir(env: str, version: "int | None" = None, root=None) -> Path:
    """The dir a WRITE targets — initializes/migrates the env if needed (creating side effects)."""
    return version_dir(env, resolve_version(env, version, root), root)


def read_dir(env: str, version: "int | None" = None, root=None) -> Path:
    """The dir a READ targets — NEVER creates. Explicit version → its dir; else the current version if
    meta exists; else the legacy flat ``<env>/`` (so a pre-migration or absent pool reads as empty
    instead of being conjured into existence — keeps fail-soft readers side-effect-free)."""
    if version is not None:
        return version_dir(env, version, root)
    cur = current_version(env, root)
    return version_dir(env, cur, root) if cur is not None else env_root(env, root)


def ratings_file(env: str, version: "int | None" = None, root=None) -> Path:
    """Path to a version's ``ratings.json`` without creating anything (for fail-soft readers)."""
    return read_dir(env, version, root) / "ratings.json"


# ── fingerprint comparison (pure; the live one is computed by the caller) ───────────────────────────
def fingerprints_compatible(stored: "dict | None", live: dict) -> bool:
    """Do two contract fingerprints agree on what makes ratings comparable — the obs shapes and the
    action dim? (The engine git sha is provenance only, NOT part of the guard.) An empty/absent stored
    fingerprint is treated as 'unknown' → compatible (the caller backfills it)."""
    if not stored:
        return True
    return (stored.get("obs_spec") == live.get("obs_spec")
            and stored.get("action_dim") == live.get("action_dim"))


# ── registry (agents.json) ───────────────────────────────────────────────────────────────────────
@dataclass
class AgentEntry:
    """One registered agent: ``kind`` = adapter name, ``checkpoint`` = weights path (or None)."""

    name: str
    kind: str
    checkpoint: "str | None" = None
    notes: str = ""


def load_registry(env: str, root=None, version: "int | None" = None) -> "dict[str, AgentEntry]":
    p = read_dir(env, version, root) / "agents.json"     # read: never creates
    if not p.exists():
        return {}
    raw = json.loads(p.read_text())
    return {name: AgentEntry(name=name, **d) for name, d in raw.items()}


def save_registry(env: str, registry: "dict[str, AgentEntry]", root=None,
                  version: "int | None" = None) -> None:
    d = active_dir(env, version, root)
    d.mkdir(parents=True, exist_ok=True)
    out = {name: {k: v for k, v in asdict(e).items() if k != "name"}
           for name, e in registry.items()}
    _atomic_write(d / "agents.json", json.dumps(out, indent=2, sort_keys=True) + "\n")


def register_agent(env: str, name: str, kind: str, checkpoint: "str | None" = None,
                   notes: str = "", *, root=None, version: "int | None" = None,
                   overwrite: bool = False) -> AgentEntry:
    """Add (or, with ``overwrite``, replace) an agent in the (current) env registry."""
    reg = load_registry(env, root, version)
    if name in reg and not overwrite:
        raise ValueError(f"agent {name!r} already registered in {env!r} "
                         f"(pass overwrite=True to replace)")
    entry = AgentEntry(name=name, kind=kind, checkpoint=checkpoint, notes=notes)
    reg[name] = entry
    save_registry(env, reg, root, version)
    return entry


# ── crosstable (games.jsonl) ─────────────────────────────────────────────────────────────────────
@dataclass
class GameRow:
    """One Arena batch: ``a`` (seat 0) played ``b`` for ``n`` games in mode ``mode`` at ``seed``."""

    a: str
    b: str
    a_wins: int
    b_wins: int
    draws: int
    n: int
    seed: int
    mode: str = "greedy"


def append_games(env: str, rows: "GameRow | list[GameRow]", root=None,
                 version: "int | None" = None) -> None:
    """Append crosstable rows (JSONL, one object per line) to the CURRENT version. Refuses to write
    into a historical version — old contracts are read-only (re-fit, never re-played)."""
    if isinstance(rows, GameRow):
        rows = [rows]
    cur = ensure_initialized(env, root)
    v = resolve_version(env, version, root)
    if v != cur:
        raise ValueError(f"refusing to append games into {env} v{v} — only the current version "
                         f"(v{cur}) accepts new games (bump-env opens a new one)")
    d = version_dir(env, v, root)
    d.mkdir(parents=True, exist_ok=True)
    with (d / "games.jsonl").open("a") as f:
        for r in rows:
            f.write(json.dumps(asdict(r)) + "\n")


def load_games(env: str, root=None, version: "int | None" = None) -> "list[GameRow]":
    p = read_dir(env, version, root) / "games.jsonl"      # read: never creates
    if not p.exists():
        return []
    out = []
    for line in p.read_text().splitlines():
        line = line.strip()
        if line:
            out.append(GameRow(**json.loads(line)))
    return out


def aggregate(rows: "list[GameRow]", names: "list[str] | None" = None):
    """Aggregate crosstable rows into ``(names, W, N)``.

    ``W[i,j]`` = 'wins' of *i* over *j* with draws counted as half to each side; ``N[i,j]`` = total
    games between *i* and *j* (symmetric). ``names`` is the sorted union of all agents seen (plus any
    passed in), so registered-but-unplayed agents can be included by passing them explicitly.
    """
    seen = set(names or [])
    for r in rows:
        seen.update((r.a, r.b))
    names = sorted(seen)
    idx = {n: i for i, n in enumerate(names)}
    A = len(names)
    W = np.zeros((A, A), dtype=np.float64)
    N = np.zeros((A, A), dtype=np.float64)
    for r in rows:
        i, j = idx[r.a], idx[r.b]
        half = 0.5 * r.draws
        W[i, j] += r.a_wins + half
        W[j, i] += r.b_wins + half
        tot = r.a_wins + r.b_wins + r.draws
        N[i, j] += tot
        N[j, i] += tot
    return names, W, N


# ── the Bradley-Terry penalized-MLE fit (numpy Newton) ─────────────────────────────────────────────
def fit_bradley_terry(names, W, N, *, anchor=ANCHOR_NAME, anchor_rating=ANCHOR_RATING,
                      reg=1e-3, tol=1e-10, max_iter=200):
    """Fit BT strengths by penalized MLE; return ``(theta, se_theta, info)``.

    Maximizes ``sum_ij W_ij·log sigmoid(theta_i - theta_j) - (reg/2)·sum theta^2`` by Newton's
    method (concave → global optimum, deterministic from ``theta=0``), pinning ``theta[anchor]=0``.
    ``se_theta`` from the inverse Fisher information (the ridge guarantees it exists). All in theta
    space; the caller maps to Elo. ``info`` carries ``iters`` and ``grad_norm`` for diagnostics.
    """
    names = list(names)
    A = len(names)
    if A == 0:
        return np.zeros(0), np.zeros(0), {"iters": 0, "grad_norm": 0.0}
    if anchor not in names:
        raise ValueError(f"anchor {anchor!r} not among agents {names}")
    a_idx = names.index(anchor)
    free = [i for i in range(A) if i != a_idx]

    theta = np.zeros(A, dtype=np.float64)
    grad_norm = 0.0
    it = 0
    for it in range(1, max_iter + 1):
        d = theta[:, None] - theta[None, :]          # d[i,j] = theta_i - theta_j
        P = _sigmoid(d)
        g_full = (W - N * P).sum(axis=1) - reg * theta
        w = N * P * (1.0 - P)                          # symmetric edge weights
        H_full = np.diag(w.sum(axis=1)) - w           # +Laplacian = Fisher(LL)
        H_full = H_full + reg * np.eye(A)             # Fisher of the penalized objective
        if not free:
            break
        Ff = H_full[np.ix_(free, free)]
        gf = g_full[free]
        grad_norm = float(np.max(np.abs(gf)))
        if grad_norm < tol:
            break
        theta[free] += np.linalg.solve(Ff, gf)        # Newton: Fisher · delta = grad

    se = np.zeros(A, dtype=np.float64)
    if free:
        d = theta[:, None] - theta[None, :]
        P = _sigmoid(d)
        w = N * P * (1.0 - P)
        Fisher = (np.diag(w.sum(axis=1)) - w + reg * np.eye(A))[np.ix_(free, free)]
        cov = np.linalg.inv(Fisher)
        se_free = np.sqrt(np.clip(np.diag(cov), 0.0, None))
        for k, i in enumerate(free):
            se[i] = se_free[k]
    return theta, se, {"iters": it, "grad_norm": grad_norm}


def _sigmoid(x):
    return 0.5 * (1.0 + np.tanh(0.5 * x))              # numerically stable logistic


def nontransitivity(names, W, N, theta, *, top=5):
    """Largest gaps between the BT-predicted and observed pairwise winrates.

    For each unordered pair with games: ``observed = W[i,j]/N[i,j]``, ``predicted =
    sigmoid(theta_i - theta_j)``, ``residual = observed - predicted``. Returns ``{max_residual,
    pairs: [...]}`` sorted by ``abs(residual)`` desc.
    """
    A = len(names)
    pairs = []
    for i in range(A):
        for j in range(i + 1, A):
            if N[i, j] <= 0:
                continue
            obs = W[i, j] / N[i, j]
            pred = float(_sigmoid(np.array(theta[i] - theta[j])))
            pairs.append({
                "a": names[i], "b": names[j], "n": int(round(N[i, j])),
                "observed": round(obs, 4), "predicted": round(pred, 4),
                "residual": round(obs - pred, 4),
            })
    pairs.sort(key=lambda p: abs(p["residual"]), reverse=True)
    return {"max_residual": pairs[0]["residual"] if pairs else 0.0, "pairs": pairs[:top]}


# ── the top-level: fit and write ratings.json ──────────────────────────────────────────────────────
def compute_ratings(env: str, *, root=None, version: "int | None" = None, reg=1e-3,
                    include_registered=True):
    """Load one version's crosstable (+ registry), fit BT, return the ratings dict (does not write).

    Carries the version's ``env_version`` and contract ``fingerprint``. ``include_registered`` folds
    registered-but-unplayed agents in (``games=0``, wide CI). The anchor is always included.
    """
    ver = version if version is not None else (current_version(env, root) or 1)   # read: no create
    meta = load_meta(env, root) or {"versions": {}}
    fingerprint = meta.get("versions", {}).get(str(ver), {}).get("fingerprint", {})
    rows = load_games(env, root, version)
    extra = list(load_registry(env, root, version).keys()) if include_registered else []
    extra.append(ANCHOR_NAME)
    names, W, N = aggregate(rows, names=extra)

    head = {"env": env, "env_version": ver, "fingerprint": fingerprint, "anchor": ANCHOR_NAME,
            "anchor_rating": ANCHOR_RATING, "scale": round(ELO_SCALE, 4), "reg": reg,
            "n_rows": len(rows)}
    if ANCHOR_NAME not in names:
        return {**head, "n_games_total": 0, "agents": {},
                "nontransitivity": {"max_residual": 0.0, "pairs": []}}

    theta, se, info = fit_bradley_terry(names, W, N, anchor=ANCHOR_NAME,
                                        anchor_rating=ANCHOR_RATING, reg=reg)
    agents = {}
    for i, name in enumerate(names):
        rating = ANCHOR_RATING + ELO_SCALE * theta[i]
        half = ELO_SCALE * 1.96 * se[i]
        agents[name] = {
            "rating": round(float(rating), 1),
            "ci_lo": round(float(rating - half), 1),
            "ci_hi": round(float(rating + half), 1),
            "games": int(round(float(N[i].sum()))),
            "wins": round(float(W[i].sum()), 1),
            "draws": int(round(float(sum(_row_draws(rows, name))))),
            "theta": round(float(theta[i]), 4),
            "se": round(float(se[i]), 4),
        }
    return {**head, "n_games_total": int(round(N.sum() / 2)), "fit": info,
            "agents": agents, "nontransitivity": nontransitivity(names, W, N, theta)}


def _row_draws(rows, name):
    for r in rows:
        if r.a == name or r.b == name:
            yield r.draws


def write_ratings(env: str, *, root=None, version: "int | None" = None, reg=1e-3) -> dict:
    """Recompute a version's ratings from its crosstable and (re)write its ``ratings.json``."""
    ver = resolve_version(env, version, root)
    out = compute_ratings(env, root=root, version=ver, reg=reg)
    d = version_dir(env, ver, root)
    d.mkdir(parents=True, exist_ok=True)
    _atomic_write(d / "ratings.json", json.dumps(out, indent=2, sort_keys=True) + "\n")
    return out


def format_table(ratings: dict) -> str:
    """A compact human-readable ratings table (highest first), with CIs and game counts."""
    agents = ratings.get("agents", {})
    ver = ratings.get("env_version", "?")
    if not agents:
        return f"[{ratings.get('env','?')} v{ver}] no rated agents yet"
    rows = sorted(agents.items(), key=lambda kv: kv[1]["rating"], reverse=True)
    w = max((len(n) for n in agents), default=5)
    lines = [f"── ratings [{ratings['env']} v{ver}]  (anchor {ratings['anchor']}="
             f"{ratings['anchor_rating']:.0f}, {ratings['n_games_total']} games) ──",
             f"  {'agent':<{w}}  {'rating':>7}  {'95% CI':>17}  {'games':>6}"]
    for name, r in rows:
        ci = f"[{r['ci_lo']:.0f}, {r['ci_hi']:.0f}]"
        lines.append(f"  {name:<{w}}  {r['rating']:>7.1f}  {ci:>17}  {r['games']:>6}")
    nt = ratings.get("nontransitivity", {})
    if nt.get("pairs"):
        p = nt["pairs"][0]
        lines.append(f"  worst nontransitivity: {p['a']} vs {p['b']} "
                     f"observed={p['observed']:.2f} predicted={p['predicted']:.2f} "
                     f"(residual {p['residual']:+.2f})")
    return "\n".join(lines)
