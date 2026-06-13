# CLAUDE.md — mtgenv

Guidance for AI agents (and humans) working in this repo. Read this first.

## What this is

`mtgenv` is a **from-scratch Rust implementation of the Magic: The Gathering rules
engine**, intended to be the fast, headless simulation core of a **Gymnasium RL
environment** for training an MTG AI in Python + PyTorch (self-play). Long-term it should
implement the full ruleset; near-term it's a tiny card pool with a correct core.

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

## Repo conventions

- **Maintain a linear git history** (rebase, not merge; `--force-with-lease`).
- **Auto-commit** meaningful progress (no need to ask). Keep messages to a short
  one-line summary, human-style. **No `Co-Authored-By` trailer** (overrides global default).
- **Keep the trackers current, without being asked:** append to `WORKLOG.md` (short dated
  entries) and update `PROJECT_STATE.md` (goals + current state) whenever you make
  meaningful progress.
- Binaries live in their own thin crates/`bin/` with minimal build targets; one canonical
  import path per item (no re-export shims). Build artifacts (`target/`) are gitignored.
- Test what you write: `cargo build` / `cargo test` (workspace).

## Prior art on this machine (reference / reuse — don't reinvent)

- `../forge-ai` — **Forge**, a complete open-source Java MTG engine the user works with.
  The reference for: the `PlayerController` decision pattern (`forge-game/.../player/
  PlayerController.java`), an existing RL bridge (`PlayerControllerRl` + `RlBridge` JSON-
  over-TCP:12345 ↔ `python/ForgeEnv`), and a huge declarative card corpus
  (`forge-gui/res/cardsfolder/`). Use Forge as a **differential-testing oracle** and a
  possible **interim Gym backend**.
- `../magician` — 17lands-style MTGA replay/draft-data ML (state/feature representation ideas).
- `../../from-scratch/mtgai` — an earlier MTG-AI attempt (notes + a Forge PlayerController).
- `../mtga-re` — **(not yet created)** target repo for the MTGA decompile work.

## Key facts already established

- MTGA is a **Mono** Unity build (not IL2CPP) installed via Steam at
  `~/.local/share/Steam/steamapps/common/MTGA/`; its protocol is **protobuf**
  (`Wizards.MDN.GreProtobuf.dll`) with `GREToClientMessage`/`ClientToGREMessage`/
  `GameStateMessage` and a request catalog (Mulligan/DeclareAttackers/SelectTargets/Order/
  PayCosts/AssignDamage/…). The user already captures the Detailed-Logs GRE stream.
- Target the paper Comprehensive Rules as truth; keep an "Arena profile" for MTGA-specific
  behavior (London mulligan, Bo1/Bo3, exact decision points).
