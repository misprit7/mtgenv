# CLAUDE.md — mtgenv

Guidance for AI agents (and humans) working in this repo. Read this first.

## What this is

`mtgenv` is a **from-scratch Rust implementation of the Magic: The Gathering rules
engine**, intended to be the fast, headless simulation core of a **Gymnasium RL
environment** for training an MTG AI in Python + PyTorch (self-play). Long-term it should
implement the full ruleset; near-term it's a tiny card pool with a correct core.

## Setup

After cloning, run `./scripts/setup.sh` once (idempotent). It downloads Scryfall's
`oracle_cards` bulk data into the gitignored `data/scryfall/` and installs the web client's
npm deps. Then `cargo build` / `cargo test`.

## Read order (the docs are the spec & the plan)

1. `docs/design/WHITEBOARD_MODEL.md` — **the architecture.** The engine is modeled on MTG
   Arena's own "whiteboard" GRE design. This is a deliberate, load-bearing decision.
2. `docs/rules/RULES_SUMMARY.md` — engine-implementer's map of the Comprehensive Rules,
   with CR rule numbers. The raw rules are in `docs/rules/comprules.txt` (grep by number,
   e.g. `613.` for the layer system) and `docs/rules/MagicCompRules_20260227.pdf`.
3. `docs/plans/ENGINE_PLAN.md` — how we build the Rust engine (crates, milestones,
   testing, the agent boundary).
4. `docs/plans/GYM_PLAN.md` — Rust↔Python boundary (PyO3+maturin), Gym env, action
   masking, training.
5. `docs/plans/DECOMPILE_PLAN.md` — recovering MTGA's client↔server decision protocol so
   the agent boundary can mirror it (work happens in a separate repo, see below).

## Architecture law (do not violate without updating WHITEBOARD_MODEL.md)

- **The core engine is card-agnostic.** It must never `match` on card names or
  card-specific behavior. All card behavior is *data* (the Effect IR) interpreted by the
  effect runtime. Card-specific logic that can't be expressed in the IR uses the
  `Native` escape hatch — never a special case in the core.
- **One decision boundary.** Every player choice flows through a single `Agent` trait with
  a `DecisionRequest`/`DecisionResponse` enum where the engine pre-enumerates the *legal*
  options (masking is the engine's job). Backends — scripted AI, Python RL, future MTGA
  client — are interchangeable implementations. This is the project's "easy switch" goal.
- **Headless core.** `mtg-core` has no GUI/Python/IO deps. GUI/bindings/CLI are separate
  crates.

## Scope — first pass (keep it minimal, get it hand-playable)

Do **not** try to implement the whole Comprehensive Rules up front. The near-term goal is a
**hand-testable playable game, soon**: basic lands + mana, casting vanilla creatures and a few
simple instants/sorceries (deal damage, draw, gain life), combat (attack / block / damage),
and win/lose. Get that working and playable by hand (CLI then web) before adding depth.

**Defer — do NOT implement in the first pass:** keyword abilities/mechanics (flying, trample,
deathtouch, …), the full layer system (CR 613) beyond trivial P/T, complex
replacement/prevention interactions, double-faced / flip / split / adventure / leveler cards,
pile/complex-choice cards, mulligans beyond "keep", multiplayer (CR 800s), and variants. The
architecture must keep these *possible later* (whiteboard + Effect IR + `Native`) — but leave
them unbuilt for now. When in doubt, pick the smaller slice that lets a human play and verify
basic functionality.

## Card data

When authoring a card, **look up its oracle text / cost / types on Scryfall** — do not guess
or rely on memory for what a card does. API: `https://api.scryfall.com/cards/named?exact=<name>`
(fields: `oracle_text`, `mana_cost`, `type_line`, `power`/`toughness`, `oracle_id`). Query cards
individually for now; later we'll ingest Scryfall's bulk `oracle-cards`/`default-cards` data +
images. Cards are *data* (Characteristics + Effect-IR abilities); the core never matches on a
card's name.

## Repo conventions

- **Maintain a linear git history** (rebase, not merge; `--force-with-lease`).
- **Auto-commit** meaningful progress (no need to ask) as a short one-line human-style
  summary; **no `Co-Authored-By` trailer** (overrides global default). Commit *working
  in-progress snapshots* at natural increments — a module compiles, a sub-feature works or
  its tests pass — so there are several small commits between "nothing" and a finished
  feature, never one giant commit. Keep it sparse (not every line), and every commit builds.
- **Keep the trackers current, without being asked:** append to `WORKLOG.md` (short dated
  entries) and update `PROJECT_STATE.md` (goals + current state) whenever you make
  meaningful progress.
- Binaries live in their own thin crates/`bin/` with minimal build targets; one canonical
  import path per item (no re-export shims). Build artifacts (`target/`) are gitignored.
- Test what you write: `cargo build` / `cargo test` (workspace).
- **Inline expect-style tests for functionality.** Use the `expect-test` crate — the Rust
  analog of Jane Street's `ppx_expect` — co-located in `#[cfg(test)] mod tests`. For anything
  with meaningful output (a rendered game state, the enumerated legal options at a decision
  point, a turn trace, a serialized message), snapshot it:
  `expect![[r#"...expected..."#]].assert_eq(&actual)` over a `Debug`/`Display` render, and
  regenerate the expected blocks with `UPDATE_EXPECT=1 cargo test`. `expect-test` is in
  `[workspace.dependencies]`; add `expect-test.workspace = true` to your crate's
  `[dev-dependencies]`. Plain `assert!`/`assert_eq!` are fine for simple invariants, but
  prefer expect snapshots for behaviour/functionality.

## Prior art on this machine (reference / reuse — don't reinvent)

- `../forge-ai` — **Forge** (open-source Java MTG engine). This is the **prior attempt that
  `mtgenv` replaces**, and it was painful to work with. Do **NOT** align the design to Forge,
  use it as a testing oracle, depend on its API, or import its card scripts — treat it as
  off-limits prior art, not a reference. (Validate rules via CR-derived expect tests + the
  captured MTGA Detailed-Logs; source card data from MTGJSON/Scryfall.)
- `../magician` — 17lands-style MTGA replay/draft-data ML (state/feature representation ideas).
- `../../from-scratch/mtgai` — an earlier MTG-AI attempt (notes).
- `../mtga-re` — **(not yet created)** target repo for the MTGA decompile work.

## Key facts already established

- MTGA is a **Mono** Unity build (not IL2CPP) installed via Steam at
  `~/.local/share/Steam/steamapps/common/MTGA/`; its protocol is **protobuf**
  (`Wizards.MDN.GreProtobuf.dll`) with `GREToClientMessage`/`ClientToGREMessage`/
  `GameStateMessage` and a request catalog (Mulligan/DeclareAttackers/SelectTargets/Order/
  PayCosts/AssignDamage/…). The user already captures the Detailed-Logs GRE stream.
- Target the paper Comprehensive Rules as truth; keep an "Arena profile" for MTGA-specific
  behavior (London mulligan, Bo1/Bo3, exact decision points).
