"""LLM ladder bot — a frontier LLM plays the gym env through the SAME obs/mask contract as every
other agent.

The observation tensors (contract v3: globals / bf / hand / stack / edges / choice) are rendered to
a faithful plaintext board — every populated field is present, nothing is added that isn't in the
obs (card NAMES aren't in the obs, only hashed identities + characteristics, so cards read like
"2/2 G creature [id h1234]"). The legal actions from the factored mask are listed with meanings;
the LLM answers with one action index; the env applies it.

Token thrift: decisions with exactly ONE legal action are auto-taken (no LLM call) — that removes
most priority windows. Prompts are compact (present rows only, terse fields).

Backends:
  --backend cli   (default) shell out to the `claude` CLI (`claude -p`) — uses the local Claude
                  Code login, no API key needed. Slow (~seconds/call) but zero setup.
  --backend api   Anthropic API via the `anthropic` package; needs ANTHROPIC_API_KEY.

ALWAYS run `--estimate` first: it plays N stand-in games (random legal moves, NO LLM calls),
renders every prompt it *would* have sent, and prints a per-game / per-run token + cost budget.

Examples:
  python llm_agent.py --deck swine --estimate --games 5
  python llm_agent.py --deck swine --games 1 --backend cli --model claude-sonnet-5 --max-llm-calls 3
  python llm_agent.py --deck swine --games 10 --backend api --model claude-sonnet-5
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time

import numpy as np

from mtgenv_gym import MtgEnv

# ── contract layout (derived from the live engine like evalkit.scripted does) ──────────────────
import mtg_py

SPEC = {name: (rows, cols) for (name, rows, cols, _i) in mtg_py.PyGame.obs_spec()}
MAX_HAND, F_HAND = SPEC["hand_feat"]
MAX_PERM, F_PERM = SPEC["bf_feat"]
MAX_STACK, F_STACK = SPEC["stack_feat"]
ACTION_DIM = int(mtg_py.PyGame.action_dim())
COMMIT = 0
HAND_BASE = 1
PERM_BASE = HAND_BASE + MAX_HAND
PLAYER_BASE = PERM_BASE + MAX_PERM
STACK_BASE = PLAYER_BASE + 2
MODE_BASE = STACK_BASE + MAX_STACK
COLOR_BASE = MODE_BASE + 16
NUMBER_BASE = COLOR_BASE + 5
NO = ACTION_DIM - 1
YES = ACTION_DIM - 2

PHASES = ["Untap", "Upkeep", "Draw", "PrecombatMain", "BeginCombat", "DeclareAttackers",
          "DeclareBlockers", "CombatDamage", "EndCombat", "PostcombatMain", "End", "Cleanup"]
REQUESTS = ["ChooseStartingPlayer", "Mulligan", "Priority", "ChooseModes", "ChooseNumber",
            "CastingTimeOptions", "ChooseTargets", "Distribute", "PayCost", "DeclareAttackers",
            "DeclareBlockers", "AssignCombatDamage", "OrderObjects", "SelectCards",
            "SelectFromGroups", "ArrangeCards", "ChooseReplacement", "ChooseCounterType",
            "ChooseOption", "ChooseColor", "Confirm"]
CARD_TYPES = ["Creature", "Land", "Artifact", "Enchantment", "Planeswalker", "Instant", "Sorcery", "Battle"]
COLORS = ["W", "U", "B", "R", "G"]
KEYWORDS = ["Deathtouch", "Defender", "DoubleStrike", "FirstStrike", "Flash", "Flying", "Haste",
            "Hexproof", "Indestructible", "Lifelink", "Menace", "Reach", "Trample", "Vigilance", "Ward"]
EDGE_NAMES = ["BLOCKS", "ATTACKS", "ATTACHED_TO", "TARGETS", "STACK_SOURCE", "PENDING_PICK"]
G_MY, G_OPP = 16, 29  # per-seat globals blocks: [life poison hand lib gy exile bf W U B R G C]

# Very-rough public API prices, $/Mtok (input, output) — for the budget printout only.
PRICES = {"claude-sonnet-5": (3.0, 15.0), "claude-opus-4-8": (15.0, 75.0),
          "claude-haiku-4-5-20251001": (0.8, 4.0)}


def arr(obs, key):
    rows, cols = SPEC[key]
    return np.asarray(obs[key], dtype=np.float32).reshape(rows, cols)


def row_label(r: int) -> str:
    if r < MAX_PERM:
        return f"B{r}"
    if r < MAX_PERM + MAX_HAND:
        return f"H{r - MAX_PERM}"
    if r < MAX_PERM + MAX_HAND + MAX_STACK:
        return f"S{r - MAX_PERM - MAX_HAND}"
    return {MAX_PERM + MAX_HAND + MAX_STACK: "you",
            MAX_PERM + MAX_HAND + MAX_STACK + 1: "opp",
            MAX_PERM + MAX_HAND + MAX_STACK + 2: "decision"}.get(r, f"row{r}")


def _flags(row, base, names):
    return [n for i, n in enumerate(names) if row[base + i] > 0.5]


def _card_bits(types, colors):
    c = "".join(colors) or "C"
    t = "/".join(types) or "?"
    return f"{c} {t}"


def bf_row_text(i, row, grp):
    types = _flags(row, 9, CARD_TYPES)
    colors = _flags(row, 17, COLORS)
    kws = _flags(row, 22, KEYWORDS)
    bits = [f"B{i}:", "yours" if row[1] > 0.5 else "OPP's", _card_bits(types, colors)]
    if "Creature" in types or row[8] > 0.5:
        bits.append(f"{int(row[2])}/{int(row[3])}")
    if row[5] > 0.5:
        bits.append(f"{int(row[5])}dmg")
    for col, name in ((6, "TAPPED"), (7, "summoning-sick"), (8, "face-down"),
                      (39, "ATTACKING"), (40, "blocking"), (44, "in-my-pending-combat-plan")):
        if row[col] > 0.5:
            bits.append(name)
    if row[43] > 0.5:
        bits.append(f"blocked-by:{int(row[43])}")
    if row[37] > 0.5:
        bits.append(f"counters:{int(row[37])}")
    if kws:
        bits.append(",".join(kws))
    if row[42] > 0.5:
        bits.append("<CHOOSABLE>")
    if row[41] > 0.5:
        bits.append("<decision-source>")
    bits.append(f"[id h{int(grp)}]")
    return " ".join(bits)


def hand_row_text(i, row, grp):
    types = _flags(row, 3, CARD_TYPES)
    colors = _flags(row, 11, COLORS)
    bits = [f"H{i}:", _card_bits(types, colors), f"cost {int(row[1])}"]
    if row[2] > 0.5:
        bits.append("castable-now")
    if row[17] > 0.5:
        bits.append("<CHOOSABLE>")
    bits.append(f"[id h{int(grp)}]")
    return " ".join(bits)


def stack_row_text(i, row, grp):
    types = _flags(row, 3, CARD_TYPES)
    colors = _flags(row, 11, COLORS)
    bits = [f"S{i}:", "yours" if row[1] > 0.5 else "OPP's", _card_bits(types, colors), f"mv {int(row[2])}"]
    if row[16] > 0.5:
        bits.append("<this-is-the-deciding-spell>")
    if row[17] > 0.5:
        bits.append("<CHOOSABLE>")
    bits.append(f"[id h{int(grp)}]")
    return " ".join(bits)


def obs_to_text(obs) -> str:
    g = np.asarray(obs["globals"], dtype=np.float32).reshape(-1)
    phase = PHASES[int(np.argmax(g[1:13]))]
    req = REQUESTS[int(np.argmax(g[43:43 + 21]))]
    my, op = g[G_MY:G_MY + 13], g[G_OPP:G_OPP + 13]
    mana = lambda b: "".join(f"{c}{int(n)}" for c, n in zip("WUBRGC", b[7:13]) if n > 0) or "none"
    lines = [
        f"TURN {int(g[0])} · phase {phase} · " + ("YOUR turn" if g[13] > 0.5 else "OPPONENT'S turn"),
        f"DECISION: {req} (bounds {int(g[65])}..{int(g[66])})" if g[65] or g[66] else f"DECISION: {req}",
        f"YOU: {int(my[0])} life · hand {int(my[2])} · library {int(my[3])} · graveyard {int(my[4])} "
        f"· battlefield {int(my[6])} · floating mana {mana(my)}",
        f"OPP: {int(op[0])} life · hand {int(op[2])} · library {int(op[3])} · graveyard {int(op[4])} "
        f"· battlefield {int(op[6])}",
    ]
    if g[67] > 0.5 or g[68] > 0.5:
        lines.append("player targets legal: " + ", ".join(
            n for n, v in (("you", g[67]), ("opponent", g[68])) if v > 0.5))

    bf, bfg = arr(obs, "bf_feat"), np.asarray(obs["bf_grpid"]).reshape(-1)
    rows = [bf_row_text(i, bf[i], bfg[i]) for i in range(MAX_PERM) if bf[i][0] > 0.5]
    lines.append("BATTLEFIELD:" if rows else "BATTLEFIELD: empty")
    lines += ["  " + r for r in rows]

    hand, hg = arr(obs, "hand_feat"), np.asarray(obs["hand_grpid"]).reshape(-1)
    rows = [hand_row_text(i, hand[i], hg[i]) for i in range(MAX_HAND) if hand[i][0] > 0.5]
    lines.append("YOUR HAND:" if rows else "YOUR HAND: empty")
    lines += ["  " + r for r in rows]

    stack, sg = arr(obs, "stack_feat"), np.asarray(obs["stack_grpid"]).reshape(-1)
    rows = [stack_row_text(i, stack[i], sg[i]) for i in range(MAX_STACK) if stack[i][0] > 0.5]
    if rows:
        lines.append("STACK (top last):")
        lines += ["  " + r for r in rows]

    edges = np.asarray(obs["edges"], dtype=np.int64).reshape(SPEC["edges"])
    rels = [f"  {row_label(int(s))} {EDGE_NAMES[int(t)]} {row_label(int(d))}"
            + (f" (slot {int(k)})" if t in (3, 5) else "")
            for s, d, t, k in edges if s >= 0]
    if rels:
        lines.append("RELATIONS:")
        lines += rels

    ch = arr(obs, "choice_feat")
    copts = []
    for j in range(len(ch)):
        if ch[j][0] > 0.5:
            kind = ["mode", "color", "number", "yes/no"][int(np.argmax(ch[j][1:5]))]
            val = int(ch[j][5])
            copts.append(f"  option {j}: {kind}"
                         + (f" = {val}" if kind == "number" else f" #{val}" if kind == "mode" else
                            f" = {COLORS[val] if kind == 'color' else ('YES' if val else 'NO')}"))
    if copts:
        lines.append("ABSTRACT OPTIONS:")
        lines += copts
    return "\n".join(lines)


def action_menu(obs, mask) -> "list[tuple[int, str]]":
    bf, hand = arr(obs, "bf_feat"), arr(obs, "hand_feat")
    out = []
    for a in np.flatnonzero(mask):
        a = int(a)
        if a == COMMIT:
            d = "COMMIT / finish this decision with current picks (or pass)"
        elif HAND_BASE <= a < HAND_BASE + MAX_HAND:
            i = a - HAND_BASE
            d = f"act on hand card H{i} (play/cast it, or pick it)"
        elif PERM_BASE <= a < PERM_BASE + MAX_PERM:
            i = a - PERM_BASE
            who = "your" if bf[i][1] > 0.5 else "opponent's"
            d = f"pick battlefield object B{i} ({who}) — declare it / target it / choose it"
        elif PLAYER_BASE <= a < PLAYER_BASE + 2:
            d = "target YOURSELF" if a == PLAYER_BASE else "target the OPPONENT"
        elif STACK_BASE <= a < STACK_BASE + MAX_STACK:
            d = f"target stack object S{a - STACK_BASE}"
        elif MODE_BASE <= a < MODE_BASE + 16:
            d = f"choose abstract option {a - MODE_BASE} (see ABSTRACT OPTIONS / off-board choices)"
        elif COLOR_BASE <= a < COLOR_BASE + 5:
            d = f"choose color {COLORS[a - COLOR_BASE]}"
        elif NUMBER_BASE <= a < NUMBER_BASE + 16:
            d = f"choose number option {a - NUMBER_BASE} (see ABSTRACT OPTIONS for its value)"
        elif a == YES:
            d = "YES"
        elif a == NO:
            d = "NO"
        else:
            d = f"slot {a}"
        out.append((a, d))
    return out


SYSTEM = (
    "You are an expert Magic: The Gathering player playing a simplified game through a structured "
    "interface. You will get the game state and a numbered list of legal actions. Play to WIN the "
    "game (reach opponent life 0, don't die). Think briefly, then answer with EXACTLY one line: "
    "ACTION: <index>  — the index of your chosen legal action. No other output."
)


def build_prompt(obs, mask):
    menu = action_menu(obs, mask)
    acts = "\n".join(f"  {a}: {d}" for a, d in menu)
    return f"{obs_to_text(obs)}\n\nLEGAL ACTIONS:\n{acts}\n\nAnswer with one line: ACTION: <index>"


# ── backends ─────────────────────────────────────────────────────────────────────────────────
class CliBackend:
    """`claude -p` — rides the local Claude Code login. No key needed; slow (CLI startup per call)."""

    def __init__(self, model):
        self.model = model

    def ask(self, prompt):
        r = subprocess.run(["claude", "-p", "--model", self.model, SYSTEM + "\n\n" + prompt],
                           capture_output=True, text=True, timeout=120)
        return r.stdout.strip(), None  # no token usage reported


class ApiBackend:
    def __init__(self, model):
        import anthropic

        self.client = anthropic.Anthropic()
        self.model = model

    def ask(self, prompt):
        m = self.client.messages.create(model=self.model, max_tokens=16, system=SYSTEM,
                                        messages=[{"role": "user", "content": prompt}])
        usage = (m.usage.input_tokens, m.usage.output_tokens)
        return "".join(b.text for b in m.content if b.type == "text"), usage


class LlmPolicy:
    """The agent: auto-takes single-legal decisions; otherwise renders the prompt and asks the LLM."""

    def __init__(self, backend, max_llm_calls=None, verbose=False):
        self.backend = backend
        self.max_llm_calls = max_llm_calls
        self.verbose = verbose
        self.calls = self.skipped = self.parse_fails = 0
        self.tok_in = self.tok_out = 0  # real usage (api) or chars//4 estimate (cli)

    def act(self, obs, mask) -> int:
        legal = np.flatnonzero(mask)
        if legal.size == 1:  # forced — never spend tokens on it
            self.skipped += 1
            return int(legal[0])
        if self.max_llm_calls is not None and self.calls >= self.max_llm_calls:
            self.skipped += 1
            return int(legal[0])
        prompt = build_prompt(obs, mask)
        self.calls += 1
        text, usage = self.backend.ask(prompt)
        if usage:
            self.tok_in += usage[0]
            self.tok_out += usage[1]
        else:
            self.tok_in += (len(SYSTEM) + len(prompt)) // 4
            self.tok_out += max(len(text) // 4, 1)
        m = re.search(r"ACTION:\s*(\d+)", text)
        act = int(m.group(1)) if m else -1
        if act not in legal:
            self.parse_fails += 1
            act = int(legal[0])
        if self.verbose:
            print(f"--- prompt ---\n{prompt}\n--- reply ---\n{text}\n--- took {act} ---")
        return act


def play_games(deck, n_games, policy, seed0=777):
    wins = 0
    t0 = time.time()
    for k in range(n_games):
        env = MtgEnv(deck=deck, opponent="random", max_decisions=3000)
        obs, info = env.reset(seed=seed0 + k)
        done, won = False, False
        while not done:
            a = policy.act(obs, np.asarray(info["action_mask"], dtype=bool))
            obs, r, term, trunc, info = env.step(int(a))
            done = term or trunc
            if term:
                won = r > 0
        wins += won
        print(f"  game {k + 1}/{n_games}: {'WIN' if won else 'loss'} | llm calls so far {policy.calls} "
              f"| skipped {policy.skipped} | parse fails {policy.parse_fails}")
    dt = time.time() - t0
    return wins, dt


def estimate(deck, n_games, models, seed0=777):
    """Stand-in games (random legal choice, NO LLM): count would-be calls + prompt sizes."""
    rng = np.random.default_rng(0)
    calls, prompt_chars, decisions = 0, 0, 0
    for k in range(n_games):
        env = MtgEnv(deck=deck, opponent="random", max_decisions=3000)
        obs, info = env.reset(seed=seed0 + k)
        done = False
        while not done:
            mask = np.asarray(info["action_mask"], dtype=bool)
            legal = np.flatnonzero(mask)
            decisions += 1
            if legal.size > 1:
                calls += 1
                prompt_chars += len(SYSTEM) + len(build_prompt(obs, mask))
            obs, _r, term, trunc, info = env.step(int(rng.choice(legal)))
            done = term or trunc
    per_game_calls = calls / n_games
    per_game_tok_in = (prompt_chars / n_games) / 4
    per_game_tok_out = per_game_calls * 8
    print(f"\n=== BUDGET ({deck}, measured over {n_games} stand-in games, opponent=random) ===")
    print(f"decisions/game: {decisions / n_games:.0f} · LLM calls/game (multi-legal only): {per_game_calls:.0f}"
          f" · input tok/game ≈ {per_game_tok_in / 1000:.1f}k · output tok/game ≈ {per_game_tok_out:.0f}")
    for label, games in (("1 game", 1), ("10 games", 10), ("100 games", 100)):
        tin, tout = per_game_tok_in * games, per_game_tok_out * games
        costs = " | ".join(f"{m.split('-', 1)[1] if '-' in m else m}: ${tin / 1e6 * p_in + tout / 1e6 * p_out:.2f}"
                           for m, (p_in, p_out) in models.items())
        print(f"  {label:9s}: ≈{tin / 1e6:.2f}M in / {tout / 1000:.1f}k out → API {costs}")
    print(f"  CLI backend: no $ (Claude Code login) but ~3-6s/call → ~{per_game_calls * 4 / 60:.0f} min/game")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--deck", default="swine")
    ap.add_argument("--games", type=int, default=1)
    ap.add_argument("--estimate", action="store_true", help="budget only — NO LLM calls")
    ap.add_argument("--backend", choices=["cli", "api"], default="cli")
    ap.add_argument("--model", default="claude-sonnet-5")
    ap.add_argument("--max-llm-calls", type=int, default=None, help="cap for smoke tests")
    ap.add_argument("--verbose", action="store_true", help="print every prompt/reply")
    args = ap.parse_args()

    if args.estimate:
        estimate(args.deck, args.games, PRICES)
        return

    backend = CliBackend(args.model) if args.backend == "cli" else ApiBackend(args.model)
    pol = LlmPolicy(backend, max_llm_calls=args.max_llm_calls, verbose=args.verbose)
    wins, dt = play_games(args.deck, args.games, pol)
    print(f"\n=== {args.model} via {args.backend} on {args.deck} vs random ===")
    print(f"games {args.games} · wins {wins} ({wins / args.games:.2f}) · wall {dt / 60:.1f} min")
    print(f"llm calls {pol.calls} · skipped(single-legal) {pol.skipped} · parse fails {pol.parse_fails}")
    print(f"tokens ≈ {pol.tok_in / 1000:.1f}k in / {pol.tok_out / 1000:.1f}k out"
          + (" (estimated from chars)" if args.backend == "cli" else " (exact)"))


if __name__ == "__main__":
    main()
