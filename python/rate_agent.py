#!/usr/bin/env python
"""Agent-rating tournament runner — the "add an agent to the pool" flow.

Builds and maintains per-environment Bradley-Terry rating pools (see
``mtgenv_gym/evalkit/ratings.py``). Every pairing is played BOTH seatings through the batched evalkit
``Arena``; every game persists to the append-only crosstable; ratings re-fit from scratch each time.

Subcommands
-----------
    seed <env>            register the default pool (random + scripted [+ probes] + loadable finals)
    list <env>            show the registry + the current ratings table
    refit <env>           recompute ratings.json from games.jsonl (no games played)
    smoke <env>           a few random-vs-scripted games — prove the pipeline (NOT gated)
    tournament <env>      round-robin the registered pool, both seatings   [PLAYS MANY GAMES]
    add <env> --name …    register a new agent, play it vs the pool, refit  [PLAYS MANY GAMES]

``tournament`` / ``add`` are the heavy steps (hundreds of games per pairing). Seeds rotate past every
game already recorded and are stored in each row, so re-runs sample fresh games and stay reproducible.

    python rate_agent.py seed swine
    python rate_agent.py tournament swine --games-per-seat 100
    python rate_agent.py add swine --name 5.0-ppo --kind ppo --checkpoint /path/final.zip
"""

from __future__ import annotations

import argparse
import random
import sys

import numpy as np

from mtgenv_gym.evalkit import Arena, RandomPolicy, ScriptedPolicy
from mtgenv_gym.evalkit.ratings import (
    ANCHOR_NAME,
    AgentEntry,
    GameRow,
    append_games,
    compute_ratings,
    elo_root,
    format_table,
    load_games,
    load_registry,
    register_agent,
    write_ratings,
)
from mtgenv_gym.evalkit.scripted import (
    COMMIT,
    DECISION_ONEHOT_OFF,
    NUM_REQUESTS,
    R_DECLARE_BLOCKERS,
)

# Durable checkpoint copies rescued out of the ephemeral /tmp training pools (see PROVENANCE.txt).
# Absolute (via elo_root) so SB3's load resolves them regardless of the runner's CWD.
_CKPT = str(elo_root() / "checkpoints")


class ScriptedBlockAllPolicy(ScriptedPolicy):
    """Probe variant: the scripted reference but blocks with EVERYTHING (chumps included).

    Identical to ``ScriptedPolicy`` except at declare-blockers, where it keeps toggling blockers
    instead of committing none — the naive over-blocker, a cheap lower-bound opponent whose chump
    rate the swine analyzer is designed to expose. Trivial subclass (no change to scripted.py)."""

    def _choose(self, obs, mask) -> int:
        g = np.asarray(obs["globals"]).reshape(-1)
        ridx = int(np.argmax(g[DECISION_ONEHOT_OFF:DECISION_ONEHOT_OFF + NUM_REQUESTS]))
        if ridx == R_DECLARE_BLOCKERS:
            legal = np.flatnonzero(np.asarray(mask, dtype=bool))
            noncommit = legal[legal != COMMIT]           # a block-assignment toggle, if any left
            return int(noncommit[0]) if noncommit.size else COMMIT
        return super()._choose(obs, mask)


# ── default seed pools (per env) ───────────────────────────────────────────────────────────────────
# (name, kind, checkpoint-or-None, notes). Checkpoints are the durable rescued copies; unloadable
# ones are skipped at seed time and reported.
DEFAULT_POOLS = {
    "heralds": [
        (ANCHOR_NAME, "random", None, "uniform-random-legal — the 1000 anchor"),
        ("scripted", "scripted", None, "land>spell>attack-all>never-block reference"),
        # 4.4-ppo / 4.3-dmc finals had no recoverable weights (overwritten / never saved).
    ],
    "swine": [
        (ANCHOR_NAME, "random", None, "uniform-random-legal — the 1000 anchor"),
        ("scripted", "scripted", None, "land>spell>attack-all>never-block reference"),
        ("scripted-blockall", "scripted_blockall", None, "probe: blocks with everything (over-chumps)"),
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
    if kind in ("scripted_blockall", "scripted_block_all"):
        return ScriptedBlockAllPolicy()
    if kind in ("ppo", "sb3", "attn"):
        from mtgenv_gym.evalkit.sb3 import SB3Policy
        import mtgenv_gym.policy  # noqa: F401 — EntityExtractor, for unpickle
        if kind == "attn":
            import mtgenv_gym.attn_policy  # noqa: F401 — RelationalAttnExtractor, for unpickle
        if not entry.checkpoint:
            raise ValueError(f"agent {entry.name!r} kind={kind} has no checkpoint path")
        return SB3Policy.load(entry.checkpoint, device=device)
    if kind == "dmc":
        raise NotImplementedError(
            f"agent {entry.name!r}: no evalkit DMC adapter and no persisted DMC weights")
    raise ValueError(f"unknown agent kind {kind!r} for {entry.name!r}")


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


# ── subcommands ──────────────────────────────────────────────────────────────────────────────────
def cmd_seed(args):
    pool = DEFAULT_POOLS.get(args.env)
    if pool is None:
        sys.exit(f"no default pool defined for env {args.env!r} (known: {sorted(DEFAULT_POOLS)})")
    registered, skipped = [], []
    for name, kind, ckpt, notes in pool:
        entry = AgentEntry(name=name, kind=kind, checkpoint=ckpt, notes=notes)
        ok, err = _try_build(entry, args.device)
        if ok:
            register_agent(args.env, name, kind, ckpt, notes, overwrite=True)
            registered.append(name)
        else:
            skipped.append((name, err))
    print(f"[{args.env}] registered {len(registered)}: {', '.join(registered)}")
    for name, err in skipped:
        print(f"  SKIPPED {name}: {err}")
    write_ratings(args.env)
    print(format_table(compute_ratings(args.env)))


def cmd_list(args):
    reg = load_registry(args.env)
    print(f"[{args.env}] registry ({len(reg)} agents):")
    for name, e in sorted(reg.items()):
        print(f"  {name:<18} kind={e.kind:<18} ckpt={e.checkpoint or '-'}")
    print(format_table(compute_ratings(args.env)))


def cmd_refit(args):
    out = write_ratings(args.env)
    print(format_table(out))


def cmd_smoke(args):
    """Prove the pipeline on the cheap: a few random-vs-scripted games, no checkpoints, no persist."""
    arena = Arena(args.env, analyzers=False)
    r = arena.play(ScriptedPolicy(), RandomPolicy(seed=0), n_games=args.games, seed=1234,
                   a_mode="greedy", b_mode="greedy", opponent_label="random")
    print(f"[{args.env}] smoke scripted vs random n={r.n_games}: "
          f"win={r.win_rate:.3f} turns={r.avg_turns:.1f} (w/l/d={r.wins}/{r.losses}/{r.draws})")


def cmd_tournament(args):
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
    entry = register_agent(args.env, args.name, args.kind, args.checkpoint, args.notes,
                           overwrite=args.overwrite)
    print(f"[{args.env}] registered {args.name} (kind={args.kind})")
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
    p = sub.add_parser("list"); p.add_argument("env"); p.set_defaults(func=cmd_list)
    p = sub.add_parser("refit"); p.add_argument("env"); p.set_defaults(func=cmd_refit)
    p = sub.add_parser("smoke"); p.add_argument("env"); p.add_argument("--games", type=int, default=6)
    p.set_defaults(func=cmd_smoke)

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
