"""Bradley-Terry agent ratings — the model-quality scalar for hill climbing.

Per-environment rating pools. Every game an agent ever plays persists in an append-only
crosstable; ratings are always recomputable from scratch (order-independent, deterministic).

Three files per environment live under ``data/elo/<env>/`` (gitignored via ``/data/``):

* ``agents.json``  — the registry: ``{name: {kind, checkpoint, notes}}``. ``kind`` names an
  evalkit ``Policy`` adapter (``random`` / ``scripted`` / ``scripted_noblock`` / ``ppo`` /
  ``attn`` / …); ``checkpoint`` is the weights path (``null`` for the torch-free baselines).
* ``games.jsonl`` — append-only, **one row per Arena batch** (one seating of one pairing):
  ``{a, b, a_wins, b_wins, draws, n, seed, mode}``. Both seatings of a pairing are two rows.
* ``ratings.json`` — the fitted output: ``{agents: {name: {rating, ci_lo, ci_hi, games, …}}}``
  plus a nontransitivity report (largest predicted-vs-observed pairwise winrate residuals).

The model is Bradley-Terry: agent *i* has strength ``theta_i`` and ``P(i beats j) =
sigmoid(theta_i - theta_j)``. We fit ``theta`` by penalized maximum likelihood (a small logistic
regression) over the AGGREGATED crosstable, with one agent (``random``) pinned so the scale is
identified, then map to an Elo-like scale ``rating = anchor_rating + (400/ln 10)·theta`` so the
anchor sits at exactly ``anchor_rating`` (1000). Draws count as half a win to each side.

Confidence intervals come from the Fisher information (the negative Hessian of the penalized
log-likelihood at the optimum): ``cov = inv(Fisher)``, ``se = sqrt(diag)``, ``ci = rating ±
1.96·(400/ln 10)·se``. A tiny ridge (``reg``) keeps ``theta`` finite and the Fisher matrix
invertible even under total separation (an agent that won or lost every game) or a disconnected
pool — such agents simply get very wide CIs, which is the honest signal.

numpy-only (no torch/sb3/scipy), so this imports in any venv that has the gym.
"""

from __future__ import annotations

import json
import math
import os
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


def elo_root(root: "str | os.PathLike | None" = None) -> Path:
    """Resolve the ``data/elo`` root. Override with ``root=`` (tests) or ``$MTGENV_ELO_ROOT``."""
    if root is not None:
        return Path(root)
    env = os.environ.get("MTGENV_ELO_ROOT")
    if env:
        return Path(env)
    # ratings.py -> evalkit -> mtgenv_gym -> python -> <repo root>
    return Path(__file__).resolve().parents[3] / "data" / "elo"


def env_dir(env: str, root=None) -> Path:
    return elo_root(root) / env


# ── registry (agents.json) ───────────────────────────────────────────────────────────────────────
@dataclass
class AgentEntry:
    """One registered agent: ``kind`` = adapter name, ``checkpoint`` = weights path (or None)."""

    name: str
    kind: str
    checkpoint: "str | None" = None
    notes: str = ""


def load_registry(env: str, root=None) -> "dict[str, AgentEntry]":
    p = env_dir(env, root) / "agents.json"
    if not p.exists():
        return {}
    raw = json.loads(p.read_text())
    return {name: AgentEntry(name=name, **d) for name, d in raw.items()}


def save_registry(env: str, registry: "dict[str, AgentEntry]", root=None) -> None:
    d = env_dir(env, root)
    d.mkdir(parents=True, exist_ok=True)
    out = {name: {k: v for k, v in asdict(e).items() if k != "name"}
           for name, e in registry.items()}
    (d / "agents.json").write_text(json.dumps(out, indent=2, sort_keys=True) + "\n")


def register_agent(env: str, name: str, kind: str, checkpoint: "str | None" = None,
                   notes: str = "", *, root=None, overwrite: bool = False) -> AgentEntry:
    """Add (or, with ``overwrite``, replace) an agent in the env registry. Returns the entry."""
    reg = load_registry(env, root)
    if name in reg and not overwrite:
        raise ValueError(f"agent {name!r} already registered in env {env!r} "
                         f"(pass overwrite=True to replace)")
    entry = AgentEntry(name=name, kind=kind, checkpoint=checkpoint, notes=notes)
    reg[name] = entry
    save_registry(env, reg, root)
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


def append_games(env: str, rows: "GameRow | list[GameRow]", root=None) -> None:
    """Append one or more crosstable rows (JSONL, one object per line). Append-only, never rewritten."""
    if isinstance(rows, GameRow):
        rows = [rows]
    d = env_dir(env, root)
    d.mkdir(parents=True, exist_ok=True)
    with (d / "games.jsonl").open("a") as f:
        for r in rows:
            f.write(json.dumps(asdict(r)) + "\n")


def load_games(env: str, root=None) -> "list[GameRow]":
    p = env_dir(env, root) / "games.jsonl"
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
        # gradient of penalized log-lik wrt every theta_i
        g_full = (W - N * P).sum(axis=1) - reg * theta
        # Hessian of penalized log-lik (negative-definite): weighted graph Laplacian - reg·I
        w = N * P * (1.0 - P)                          # symmetric edge weights
        H_full = np.diag(w.sum(axis=1)) - w           # this is +Laplacian = -Hessian_LL (PD-ish)
        H_full = H_full + reg * np.eye(A)             # Fisher (=-Hessian of penalized obj)
        if not free:
            break
        Ff = H_full[np.ix_(free, free)]               # Fisher over free params (PD)
        gf = g_full[free]
        grad_norm = float(np.max(np.abs(gf)))
        if grad_norm < tol:
            break
        step = np.linalg.solve(Ff, gf)                # Newton: Fisher · delta = grad
        theta[free] += step

    # covariance from the Fisher information at the optimum
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
    sigmoid(theta_i - theta_j)``, ``residual = observed - predicted``. A perfectly transitive pool
    fits with ~0 residuals; a large one flags a rock-paper-scissors edge the single scalar can't
    capture. Returns ``{max_residual, pairs: [...]}`` sorted by ``abs(residual)`` desc.
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
    return {
        "max_residual": pairs[0]["residual"] if pairs else 0.0,
        "pairs": pairs[:top],
    }


# ── the top-level: fit and write ratings.json ──────────────────────────────────────────────────────
def compute_ratings(env: str, *, root=None, reg=1e-3, include_registered=True):
    """Load the crosstable (and, if present, the registry), fit BT, return the ratings dict.

    Pure computation — does not write. ``include_registered`` folds registered-but-unplayed agents
    into the table (they get ``games=0`` and a wide CI). The anchor is always included.
    """
    rows = load_games(env, root)
    extra = []
    if include_registered:
        extra = list(load_registry(env, root).keys())
    extra.append(ANCHOR_NAME)
    names, W, N = aggregate(rows, names=extra)

    if ANCHOR_NAME not in names:
        # empty pool with no registry — nothing to anchor against
        return {"env": env, "anchor": ANCHOR_NAME, "anchor_rating": ANCHOR_RATING,
                "scale": ELO_SCALE, "reg": reg, "n_rows": len(rows), "n_games_total": 0,
                "agents": {}, "nontransitivity": {"max_residual": 0.0, "pairs": []}}

    theta, se, info = fit_bradley_terry(names, W, N, anchor=ANCHOR_NAME,
                                        anchor_rating=ANCHOR_RATING, reg=reg)

    agents = {}
    for i, name in enumerate(names):
        rating = ANCHOR_RATING + ELO_SCALE * theta[i]
        half = ELO_SCALE * 1.96 * se[i]
        wins = float(W[i].sum())
        games = float(N[i].sum())
        # decompose games into whole wins/losses/draws for reporting (draws are the half-fractions)
        draws_i = float(sum(_row_draws(rows, name)))
        agents[name] = {
            "rating": round(float(rating), 1),
            "ci_lo": round(float(rating - half), 1),
            "ci_hi": round(float(rating + half), 1),
            "games": int(round(games)),
            "wins": round(wins, 1),
            "draws": int(round(draws_i)),
            "theta": round(float(theta[i]), 4),
            "se": round(float(se[i]), 4),
        }

    return {
        "env": env,
        "anchor": ANCHOR_NAME,
        "anchor_rating": ANCHOR_RATING,
        "scale": round(ELO_SCALE, 4),
        "reg": reg,
        "n_rows": len(rows),
        "n_games_total": int(round(N.sum() / 2)),
        "fit": info,
        "agents": agents,
        "nontransitivity": nontransitivity(names, W, N, theta),
    }


def _row_draws(rows, name):
    for r in rows:
        if r.a == name or r.b == name:
            yield r.draws


def write_ratings(env: str, *, root=None, reg=1e-3) -> dict:
    """Recompute ratings from the crosstable and (re)write ``ratings.json``. Returns the dict."""
    out = compute_ratings(env, root=root, reg=reg)
    d = env_dir(env, root)
    d.mkdir(parents=True, exist_ok=True)
    (d / "ratings.json").write_text(json.dumps(out, indent=2, sort_keys=True) + "\n")
    return out


def format_table(ratings: dict) -> str:
    """A compact human-readable ratings table (highest first), with CIs and game counts."""
    agents = ratings.get("agents", {})
    if not agents:
        return f"[{ratings.get('env','?')}] no rated agents yet"
    rows = sorted(agents.items(), key=lambda kv: kv[1]["rating"], reverse=True)
    w = max((len(n) for n in agents), default=5)
    lines = [f"── ratings [{ratings['env']}]  (anchor {ratings['anchor']}={ratings['anchor_rating']:.0f}, "
             f"{ratings['n_games_total']} games) ──",
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
