#!/usr/bin/env python
"""Agent-rating tournament runner — the "add an agent to the pool" flow.

Builds and maintains per-environment Bradley-Terry rating pools (see
``mtgenv_gym/evalkit/ratings.py``). Every pairing is played BOTH seatings through the batched evalkit
``Arena``; every game persists to the append-only crosstable; ratings re-fit from scratch each time.

Rating environments are **versioned by the engine contract** (obs shapes × action dim × engine sha):
a change there invalidates comparability, so each contract is its own version with its own crosstable.
All commands resolve to the CURRENT version by default; ``--env-version N`` reads a historical one
(read-only). Game-playing commands guard the live engine's fingerprint against the current version's
and REFUSE on a mismatch, pointing you at ``bump-env``.

Subcommands
-----------
    seed <env>            (re)seed the current version's registry (the benchmark spine + loadable finals)
    list <env>            show the registry + ratings table [--env-version N]
    refit <env>           recompute ratings.json from games.jsonl [--env-version N] (no games played)
    smoke <env>           a few random-vs-scripted games — prove the pipeline (NOT gated)
    tournament <env>      round-robin the registered pool, both seatings   [PLAYS MANY GAMES]
    add <env> --name …    register a new agent, play it vs the pool, refit  [PLAYS MANY GAMES]
    migrate [env]         fold pre-versioning flat data into v1 (idempotent; all envs if omitted)
    bump-env <env> --reason "…"   open v(N+1) for a changed contract: fresh spine, empty crosstable

``tournament`` / ``add`` are the heavy steps (hundreds of games per pairing). Seeds rotate past every
game already recorded and are stored in each row, so re-runs sample fresh games and stay reproducible.

    python rate_agent.py migrate
    python rate_agent.py tournament swine --games-per-seat 100
    python rate_agent.py add swine --name 5.0-ppo --kind ppo --checkpoint /path/final.zip
    python rate_agent.py bump-env swine --reason "added menace keyword column to bf_feat"
"""

from __future__ import annotations

import argparse
import random
import subprocess
import sys

import numpy as np

from mtgenv_gym.evalkit import Arena, BasePolicy, RandomPolicy, ScriptedPolicy
from mtgenv_gym.evalkit.ratings import (
    ANCHOR_NAME,
    AgentEntry,
    GameRow,
    append_games,
    bump_version,
    compute_ratings,
    current_version,
    elo_root,
    ensure_initialized,
    env_root,
    fingerprints_compatible,
    format_table,
    list_versions,
    load_games,
    load_meta,
    load_registry,
    register_agent,
    save_registry,
    set_fingerprint,
    write_ratings,
)
from mtgenv_gym.evalkit.scripted import ScriptedHeuristic

# Durable checkpoint copies rescued out of the ephemeral /tmp training pools (see PROVENANCE.txt).
# Absolute (via elo_root) so SB3's load resolves them regardless of the runner's CWD.
_CKPT = str(elo_root() / "checkpoints")

# The scripted heuristic FAMILY — kind name -> (attack axis, block axis). These + random are the
# fixed benchmark spine of every env's pool (see ScriptedHeuristic).
SCRIPT_KINDS = {
    "script_racer": ("all", "never"),        # attack all-in, never block (== ScriptedPolicy)
    "script_turtle": ("never", "all"),       # never attack, block everything (the wall)
    "script_gang": ("all", "gang"),          # attack all-in, gang the biggest attacker
    "script_careful": ("conservative", "gang"),  # only safe attacks, gang blocks
}

# The benchmark spine, registered in EVERY env: random (the 1000 anchor) + the 4 named scripts.
_SPINE = [
    (ANCHOR_NAME, "random", None, "uniform-random-legal — the 1000 anchor"),
    ("script-racer", "script_racer", None, "all-in attack, never block (the racer)"),
    ("script-turtle", "script_turtle", None, "never attack, block everything (the wall)"),
    ("script-gang", "script_gang", None, "all-in attack, gang-priority blocks (2+ on the biggest)"),
    ("script-careful", "script_careful", None, "conservative attack (skip losing attacks), gang blocks"),
]

# Per-env default pool = the spine + any loadable trained finals. Unloadable finals are skipped at
# seed time and reported. (heralds 4.4-ppo/4.3-dmc finals had no recoverable weights.)
DEFAULT_POOLS = {
    "heralds": list(_SPINE),
    "swine": _SPINE + [
        ("4.6-ppo", "ppo", f"{_CKPT}/swine/4.6-ppo.zip", "4.6-ppo-swine final (ckpt_496000)"),
        ("4.7-ppo-attn", "attn", f"{_CKPT}/swine/4.7-ppo-attn.zip", "4.7-ppo-attn-swine final (ckpt_1M)"),
        ("2.9-legacy", "ppo", f"{_CKPT}/swine/2.9-legacy.zip", "2.9-swine-500k legacy PPO (ckpt_504000)"),
    ],
}


# ── policy construction from a registry entry ──────────────────────────────────────────────────────
def build_policy(entry: AgentEntry, device: str = "cpu"):
    """Map a registry entry to a live evalkit ``Policy`` (torch/sb3 imported lazily for trained kinds)."""
    kind = entry.kind
    if kind == "random":
        return RandomPolicy(seed=0)
    if kind == "scripted":
        return ScriptedPolicy()
    if kind in SCRIPT_KINDS:
        attack, block = SCRIPT_KINDS[kind]
        return ScriptedHeuristic(attack=attack, block=block)
    if kind in ("ppo", "sb3", "attn"):
        from mtgenv_gym.evalkit.sb3 import SB3Policy
        import mtgenv_gym.policy  # noqa: F401 — EntityExtractor, for unpickle
        if kind == "attn":
            import mtgenv_gym.attn_policy  # noqa: F401 — RelationalAttnExtractor, for unpickle
        if not entry.checkpoint:
            raise ValueError(f"agent {entry.name!r} kind={kind} has no checkpoint path")
        pol = SB3Policy.load(entry.checkpoint, device=device)
        return _SchemaAdapter(pol, pol.model.observation_space)  # tolerate older obs widths
    if kind == "dmc":
        raise NotImplementedError(
            f"agent {entry.name!r}: no evalkit DMC adapter and no persisted DMC weights")
    raise ValueError(f"unknown agent kind {kind!r} for {entry.name!r}")


def _fit_shape(a: np.ndarray, shape: tuple) -> np.ndarray:
    """Coerce ``a`` to ``shape`` by truncating over-long axes and zero-padding short ones."""
    if a.shape == shape:
        return a
    a = a[tuple(slice(0, s) for s in shape)]           # truncate any axis longer than target
    if a.shape != shape:                                # pad any axis shorter than target
        out = np.zeros(shape, dtype=a.dtype)
        out[tuple(slice(0, s) for s in a.shape)] = a
        a = out
    return a


class _SchemaAdapter(BasePolicy):
    """Rate an SB3 checkpoint trained on an OLDER obs schema against the current engine.

    The gym obs only ever grows by APPENDING columns (obs.rs is append-stable — e.g. bf_feat went
    44→45→48 as is_pending_combat then the relation-id columns were added, indices 0..43 unchanged).
    So a model that expects a narrower ``bf_feat`` gets fed the current obs truncated to its own
    per-key shape: exactly the features it was trained on, no more. Keys the model doesn't expect are
    dropped; a (defensive) narrower current array is zero-padded. A checkpoint already on the current
    schema passes through unchanged (``_fit_shape`` is a no-op when shapes match)."""

    def __init__(self, inner, space):
        self.inner = inner
        self.space = space

    def act(self, obs_batch, mask_batch, *, mode="greedy", env_indices=None):
        adapted = [{k: _fit_shape(np.asarray(o[k]), tuple(sp.shape))
                    for k, sp in self.space.spaces.items() if k in o}
                   for o in obs_batch]
        return self.inner.act(adapted, mask_batch, mode=mode, env_indices=env_indices)


def _try_build(entry: AgentEntry, device: str):
    try:
        build_policy(entry, device=device)
        return True, ""
    except Exception as e:  # noqa: BLE001 — report, don't crash the seed sweep
        return False, f"{type(e).__name__}: {e}"


# ── seeds ──────────────────────────────────────────────────────────────────────────────────────────
def _seed_allocator(env, seed_base, root=None):
    """A monotone seed source that starts PAST every game already recorded (fresh, reproducible)."""
    prior = sum(r.n for r in load_games(env, root))
    cur = int(seed_base) + prior
    def nxt(step):
        nonlocal cur
        s = cur
        cur += int(step)
        return s
    return nxt


# ── playing a pairing both seatings ─────────────────────────────────────────────────────────────────
def play_pairing(arena: Arena, na, pa, nb, pb, *, games_per_seat, alloc, mode) -> "list[GameRow]":
    """Play ``na`` vs ``nb`` both seatings; return the two crosstable rows (a-perspective each)."""
    s1 = alloc(games_per_seat)
    r1 = arena.play(pa, pb, n_games=games_per_seat, seed=s1, a_mode=mode, b_mode=mode,
                    opponent_label=nb)
    s2 = alloc(games_per_seat)
    r2 = arena.play(pb, pa, n_games=games_per_seat, seed=s2, a_mode=mode, b_mode=mode,
                    opponent_label=na)
    return [
        GameRow(a=na, b=nb, a_wins=r1.wins, b_wins=r1.losses, draws=r1.draws, n=r1.n_games,
                seed=s1, mode=mode),
        GameRow(a=nb, b=na, a_wins=r2.wins, b_wins=r2.losses, draws=r2.draws, n=r2.n_games,
                seed=s2, mode=mode),
    ]


def _select_opponents(new_name, ratings, all_names, *, k_top=8, k_sample=3, rng=None):
    """Opponents for an added agent: anchors + top-K + a random sample (only trims pools >12)."""
    others = [n for n in all_names if n != new_name]
    if len(others) <= 12:
        return others
    rng = rng or random.Random(0)
    keep = {ANCHOR_NAME}
    if "scripted" in others:
        keep.add("scripted")
    ranked = sorted((n for n in others), key=lambda n: ratings.get(n, {}).get("rating", 0.0),
                    reverse=True)
    keep.update(ranked[:k_top])
    rest = [n for n in others if n not in keep]
    keep.update(rng.sample(rest, min(k_sample, len(rest))))
    return [n for n in others if n in keep]


# ── engine contract fingerprint + the version guard ────────────────────────────────────────────────
def _git_sha() -> str:
    try:
        repo = str(elo_root().parent.parent)   # data/elo -> data -> <repo root>
        out = subprocess.run(["git", "-C", repo, "rev-parse", "--short", "HEAD"],
                             capture_output=True, text=True, timeout=5)
        return out.stdout.strip() or "unknown"
    except Exception:
        return "unknown"


def engine_fingerprint() -> dict:
    """The live contract fingerprint: per-key obs shapes + action_dim + engine git sha. This is the
    only engine-touching call in the rating stack; ratings.py stores/compares the dict but never
    computes it."""
    import mtg_py

    spec = {name: [rows, cols] for (name, rows, cols, _is_int) in mtg_py.PyGame.obs_spec()}
    return {"obs_spec": spec, "action_dim": int(mtg_py.PyGame.action_dim()), "engine_sha": _git_sha()}


def _spec_brief(fp: dict) -> str:
    return f"action_dim={fp.get('action_dim')} obs={fp.get('obs_spec')}"


def _guard_current_version(env: str) -> int:
    """Before any game-playing command: ensure the env is initialized (migrating flat legacy data to
    v1), record/verify the current version's fingerprint against the live engine, and REFUSE on an
    incompatible contract so stale games never mix into an invalidated pool. Returns the version."""
    live = engine_fingerprint()
    ensure_initialized(env, fingerprint=live)     # migrate/create v1; stamps live if freshly made
    ver = current_version(env)
    stored = (load_meta(env)["versions"][str(ver)].get("fingerprint")) or {}
    if not stored:                                # migrated/created without one → adopt live
        set_fingerprint(env, ver, live)
        stored = live
    if not fingerprints_compatible(stored, live):
        sys.exit(
            f"[{env} v{ver}] ENGINE CONTRACT CHANGED — ratings on this pool would be incomparable, "
            f"refusing to play.\n  stored: {_spec_brief(stored)} (sha {stored.get('engine_sha')})\n"
            f"  live:   {_spec_brief(live)} (sha {live.get('engine_sha')})\n"
            f"  Open a fresh version:  python rate_agent.py bump-env {env} --reason \"<what changed>\"\n"
            f"  (creates v{ver + 1} with an empty crosstable; v{ver} history is retained, read-only.)")
    return ver


# ── subcommands ──────────────────────────────────────────────────────────────────────────────────
def cmd_seed(args):
    pool = DEFAULT_POOLS.get(args.env)
    if pool is None:
        sys.exit(f"no default pool defined for env {args.env!r} (known: {sorted(DEFAULT_POOLS)})")
    registry, registered, skipped = {}, [], []
    for name, kind, ckpt, notes in pool:
        entry = AgentEntry(name=name, kind=kind, checkpoint=ckpt, notes=notes)
        ok, err = _try_build(entry, args.device)
        if ok:
            registry[name] = entry
            registered.append(name)
        else:
            skipped.append((name, err))
    live = engine_fingerprint()
    ver = ensure_initialized(args.env, fingerprint=live)  # init/migrate; stamp the contract
    if not (load_meta(args.env)["versions"][str(ver)].get("fingerprint")):
        set_fingerprint(args.env, ver, live)
    save_registry(args.env, registry)  # REPLACE the registry with exactly the (loadable) pool
    print(f"[{args.env} v{ver}] registered {len(registered)}: {', '.join(registered)}")
    for name, err in skipped:
        print(f"  SKIPPED {name}: {err}")
    print(format_table(write_ratings(args.env)))


def cmd_list(args):
    ver = getattr(args, "env_version", None)
    reg = load_registry(args.env, version=ver)
    cur = current_version(args.env)
    print(f"[{args.env}] versions: {sorted(list_versions(args.env))} (current v{cur}); "
          f"registry v{ver or cur} ({len(reg)} agents):")
    for name, e in sorted(reg.items()):
        print(f"  {name:<18} kind={e.kind:<18} ckpt={e.checkpoint or '-'}")
    print(format_table(compute_ratings(args.env, version=ver)))


def cmd_refit(args):
    ver = getattr(args, "env_version", None)     # read-only: may target a historical version
    print(format_table(write_ratings(args.env, version=ver)))


def cmd_smoke(args):
    """Prove the pipeline on the cheap: a few random-vs-scripted games, no checkpoints, no persist."""
    arena = Arena(args.env, analyzers=False)
    r = arena.play(ScriptedPolicy(), RandomPolicy(seed=0), n_games=args.games, seed=1234,
                   a_mode="greedy", b_mode="greedy", opponent_label="random")
    print(f"[{args.env}] smoke scripted vs random n={r.n_games}: "
          f"win={r.win_rate:.3f} turns={r.avg_turns:.1f} (w/l/d={r.wins}/{r.losses}/{r.draws})")


def cmd_tournament(args):
    ver = _guard_current_version(args.env)       # refuse if the engine contract drifted
    print(f"[{args.env}] current version v{ver}")
    reg = load_registry(args.env)
    names = [n.strip() for n in args.agents.split(",")] if args.agents else sorted(reg)
    missing = [n for n in names if n not in reg]
    if missing:
        sys.exit(f"agents not registered: {missing} (run `seed {args.env}` or `add`)")
    print(f"[{args.env}] tournament over {len(names)} agents: {', '.join(names)}")
    policies = {n: build_policy(reg[n], args.device) for n in names}
    arena = Arena(args.env, batch_size=args.batch_size, analyzers=False)
    alloc = _seed_allocator(args.env, args.seed_base)

    existing = set()
    if args.only_new:
        for row in load_games(args.env):
            existing.add(frozenset((row.a, row.b)))

    played = 0
    for i in range(len(names)):
        for j in range(i + 1, len(names)):
            na, nb = names[i], names[j]
            if args.only_new and frozenset((na, nb)) in existing:
                continue
            rows = play_pairing(arena, na, policies[na], nb, policies[nb],
                                games_per_seat=args.games_per_seat, alloc=alloc, mode=args.mode)
            append_games(args.env, rows)
            played += 1
            wa = rows[0].a_wins + rows[1].b_wins
            wb = rows[0].b_wins + rows[1].a_wins
            tot = rows[0].n + rows[1].n
            print(f"  {na} vs {nb}: {wa}-{wb} (n={tot})")
    print(f"[{args.env}] played {played} pairings")
    print(format_table(write_ratings(args.env)))


def cmd_add(args):
    ver = _guard_current_version(args.env)       # refuse if the engine contract drifted
    entry = register_agent(args.env, args.name, args.kind, args.checkpoint, args.notes,
                           overwrite=args.overwrite)
    print(f"[{args.env} v{ver}] registered {args.name} (kind={args.kind})")
    reg = load_registry(args.env)
    ratings = compute_ratings(args.env)["agents"]
    opponents = _select_opponents(args.name, ratings, list(reg),
                                  rng=random.Random(args.seed_base))
    print(f"  opponents ({len(opponents)}): {', '.join(opponents)}")
    new_pol = build_policy(entry, args.device)
    policies = {n: build_policy(reg[n], args.device) for n in opponents}
    arena = Arena(args.env, batch_size=args.batch_size, analyzers=False)
    alloc = _seed_allocator(args.env, args.seed_base)
    for nb in opponents:
        rows = play_pairing(arena, args.name, new_pol, nb, policies[nb],
                            games_per_seat=args.games_per_seat, alloc=alloc, mode=args.mode)
        append_games(args.env, rows)
        wa = rows[0].a_wins + rows[1].b_wins
        wb = rows[0].b_wins + rows[1].a_wins
        print(f"  {args.name} vs {nb}: {wa}-{wb} (n={rows[0].n + rows[1].n})")
    print(format_table(write_ratings(args.env)))


def _spine_registry(env, device):
    """Build the benchmark-spine registry (random + the 4 named scripts) for a fresh version, skipping
    any member that won't build. Trained finals are NOT auto-added — they re-join via `add`."""
    registry = {}
    for name, kind, ckpt, notes in _SPINE:
        entry = AgentEntry(name=name, kind=kind, checkpoint=ckpt, notes=notes)
        ok, _ = _try_build(entry, device)
        if ok:
            registry[name] = entry
    return registry


def cmd_bump_env(args):
    """Open a new version for a CHANGED engine contract: re-seed the benchmark spine, empty crosstable,
    stamp the live fingerprint. The current version + its games are retained (read-only)."""
    live = engine_fingerprint()
    ensure_initialized(args.env, fingerprint=live)
    old = current_version(args.env)
    registry = _spine_registry(args.env, args.device)
    n = bump_version(args.env, reason=args.reason, fingerprint=live, registry=registry)
    print(f"[{args.env}] bumped v{old} -> v{n} (reason: {args.reason!r})")
    print(f"  spine seeded: {', '.join(sorted(registry))}")
    print(f"  fingerprint: sha={live['engine_sha']} action_dim={live['action_dim']}")
    print(f"  trained agents re-join via `add`; v{old} retained (read-only).")
    print(format_table(compute_ratings(args.env, version=n)))


def cmd_migrate(args):
    """One-shot: fold pre-versioning flat data into v1 with the CURRENT fingerprint. Idempotent."""
    live = engine_fingerprint()
    envs = [args.env] if args.env else list(DEFAULT_POOLS)
    for env in envs:
        had_flat = any((env_root(env) / f).exists() for f in ("agents.json", "games.jsonl"))
        already = load_meta(env) is not None
        ver = ensure_initialized(env, fingerprint=live)
        if not (load_meta(env)["versions"][str(ver)].get("fingerprint")):
            set_fingerprint(env, ver, live)
        write_ratings(env, version=ver)          # refresh ratings.json with env_version + fingerprint
        state = "already versioned" if already else ("migrated flat data" if had_flat else "created empty")
        print(f"[{env}] {state} → current v{ver}; versions {sorted(list_versions(env))} "
              f"(sha {live['engine_sha']}, action_dim {live['action_dim']})")


def main(argv=None):
    ap = argparse.ArgumentParser(prog="rate_agent", description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    def common(p, games_default=100):
        p.add_argument("env")
        p.add_argument("--device", default="cpu")
        p.add_argument("--games-per-seat", type=int, default=games_default,
                       help="games per seating; a pairing = 2× this (both seatings)")
        p.add_argument("--seed-base", type=int, default=5_000_000)
        p.add_argument("--batch-size", type=int, default=64)
        p.add_argument("--mode", choices=("greedy", "sample"), default="greedy")

    p = sub.add_parser("seed"); p.add_argument("env"); p.add_argument("--device", default="cpu")
    p.set_defaults(func=cmd_seed)
    p = sub.add_parser("list"); p.add_argument("env")
    p.add_argument("--env-version", type=int, default=None, help="a historical version (default: current)")
    p.set_defaults(func=cmd_list)
    p = sub.add_parser("refit"); p.add_argument("env")
    p.add_argument("--env-version", type=int, default=None, help="a historical version (default: current)")
    p.set_defaults(func=cmd_refit)
    p = sub.add_parser("smoke"); p.add_argument("env"); p.add_argument("--games", type=int, default=6)
    p.set_defaults(func=cmd_smoke)

    p = sub.add_parser("migrate"); p.add_argument("env", nargs="?", default=None,
                                                  help="one env, or all default envs if omitted")
    p.add_argument("--device", default="cpu"); p.set_defaults(func=cmd_migrate)
    p = sub.add_parser("bump-env"); p.add_argument("env"); p.add_argument("--device", default="cpu")
    p.add_argument("--reason", required=True, help="what about the engine contract changed")
    p.set_defaults(func=cmd_bump_env)

    p = sub.add_parser("tournament"); common(p)
    p.add_argument("--agents", default=None, help="comma list to restrict participants")
    p.add_argument("--only-new", action="store_true", help="skip pairings already in the crosstable")
    p.set_defaults(func=cmd_tournament)

    p = sub.add_parser("add"); common(p)
    p.add_argument("--name", required=True); p.add_argument("--kind", required=True)
    p.add_argument("--checkpoint", default=None); p.add_argument("--notes", default="")
    p.add_argument("--overwrite", action="store_true")
    p.set_defaults(func=cmd_add)

    args = ap.parse_args(argv)
    args.func(args)


if __name__ == "__main__":
    main()
