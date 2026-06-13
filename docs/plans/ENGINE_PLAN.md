# Engine Implementation Plan (Rust MTG Rules Engine)

> Read first: `docs/design/WHITEBOARD_MODEL.md` (the architecture) and
> `docs/rules/RULES_SUMMARY.md` (the spec, with CR rule numbers).
> Sibling plans: `docs/plans/DECOMPILE_PLAN.md` (MTGA protocol), `docs/plans/GYM_PLAN.md`
> (Python training env).

This plan describes how to build `mtgenv` as a full, fast, headless MTG rules engine
suitable as the simulation core of an RL training environment, built on MTGA's whiteboard
model.

---

## 1. Goals & non-goals

**Goals**
- Correct implementation of the MTG Comprehensive Rules core (priority, stack, layers,
  SBAs, replacement/triggered effects, combat) — the parts that are card-*agnostic*.
- A card pool that grows as **data** (the Effect IR), starting tiny.
- **Headless, deterministic, fast, cloneable** game state — the RL env needs millions of
  game steps and cheap state snapshots (MCTS/AlphaZero).
- A single **decision/agent boundary** so the decision-maker is a pluggable
  implementation: scripted AI, Python RL agent, or (later) the real MTGA client. This is
  the user's "extremely easy switch" requirement.
- Structured so MTGA-grade complex cards become *possible later* without a rewrite
  (whiteboard + IR + `Native` escape hatch).

**Non-goals (near-term)**
- The full ~30k card pool, complex cards (Zurgo/Sylvan Library class), multiplayer
  (CR 800s), variants (CR 900s), a polished GUI, networking to the real client.

---

## 2. Design requirements (what the structure must get right)

A few structural requirements shape everything below:

- Separate an object's *copiable* base characteristics from its *computed* (post-layer)
  characteristics — required for the layer system and copy effects (CR 613/707).
- Stable object identity (`ObjId`) with zones as first-class `ObjId` lists; never clone
  cards around — keeps snapshot/clone cheap for search and replay.
- `DecisionRequest`s are generated from legal-action enumeration with constraints (masking
  is the engine's job) — the engine never asks an open-ended question.
- The phase/step model carries the full priority + turn-based-action machinery.
- `mtg-core` is headless: no GUI/Python/IO deps (GUI/bindings/CLI are separate crates).

## 3. Workspace & crate layout

Per repo conventions (headless core; binaries get their own minimal target in `bin/`; one
canonical import path; no re-export shims):

```
mtgenv/                      (cargo workspace)
  crates/
    mtg-core/        # the rules engine library (the GRE). NO GUI, NO python, NO I/O.
      src/
        ids.rs           # ObjId, PlayerId, ZoneId, Timestamp — stable identities
        state/           # GameState, Player, Zones, Object, characteristics cache
        chars/           # characteristic computation: base + layers (CR 613)
        stack.rs         # the stack (CR 405)
        turn/            # phase/step machine, turn-based actions (CR 500s)
        priority.rs      # priority loop + the agenda pipeline (CR 117.5/603/704)
        whiteboard.rs    # Action, Whiteboard, commit, event emission (design doc §2.1)
        events.rs        # Event bus, LKI snapshots (CR 603.10)
        effects/         # Effect IR: abilities, costs, targets, conditions, durations
        replacement.rs   # replacement/prevention rewrite pass (CR 614/615/616)
        triggers.rs      # triggered-ability collection & ordering (CR 603)
        sba.rs           # state-based actions (CR 704)
        combat/          # combat phase (CR 506-511)
        mana.rs          # mana, mana abilities, costs (CR 106/605/118)
        agent.rs         # the DecisionRequest/Response boundary (the trait) — §6
        cards/           # hand-authored card data in the IR (the starter pool)
        rng.rs           # seedable, replayable RNG
    mtg-cli/         # bin: headless sim runner (self-play, scripted matches) — thin
    mtg-py/          # bin/lib: PyO3 bindings for the gym (see GYM_PLAN.md) — separate crate
    mtg-viewer/      # bin: optional TUI/egui spectator — separate, depends on mtg-core only
  bin/                 # (per convention) thin entrypoints if not using crate bins
  docs/ …
  Cargo.toml           # [workspace]
```

`mtg-core` is the only crate that matters for correctness and is the only dependency of
the bindings/CLI/viewer. It must compile and run with zero GUI/Python deps. The current
`egui`/`eframe`/`serde_json` deps move out of core (`serde` stays for state
serialization).

## 4. The card-agnostic core (the GRE)

Implements only structural rules; **never matches on card identity** (enforced by the
crate boundary — card behavior lives in the IR data, interpreted here). Key pieces, each
with its CR anchor (see RULES_SUMMARY.md):

- **State & zones** (CR 400, 108–112): seven zones, public/hidden, ownership vs control,
  objects with stable `ObjId`, status (tapped/flipped/face-down/phased), counters.
- **Characteristics** (CR 109/200s + 613): `base (copiable) ⊕ layers → computed cache`,
  recomputed on dirty. The cache is the single source of truth queried everywhere.
- **Turn machine** (CR 500s): the phase/step table from RULES_SUMMARY §turn-structure;
  turn-based actions fire at the right beats; priority granted per CR 117.
- **Priority + agenda loop** (CR 117.5, 603.3, 704.3): the fixpoint in WHITEBOARD_MODEL
  §2.2 — recompute → SBAs (loop) → triggers on stack (APNAP) → priority.
- **The stack** (CR 405, 608): put on, respond, resolve; illegal-target-on-resolution
  handling (CR 608.2b).
- **Casting/activating** (CR 601.2a–i, 602, 605): the detailed cast sequence as a
  reusable routine that produces choice requests (modes, targets, costs, X) via the agent
  boundary, then a stack object.
- **Whiteboard commit** (design doc §2.1): materialize → replacement/prevention rewrite
  pass → commit → emit events.
- **Combat** (CR 506–511): begin/attackers/blockers/damage/end; damage assignment order,
  first/double strike substeps, trample, removal-from-combat. Generates the
  declare-attackers/declare-blockers/assign-damage decision requests.
- **Mana & costs** (CR 106/605/118): mana pool, mana abilities (don't use the stack),
  additional/alternative costs, cost modification (via qualifications).

## 5. The effect runtime & card data (the CLIPS + GRP layer)

- **Effect IR** (`effects/`): the enum vocabulary of atomic effects, cost types, target
  selectors, conditions, durations, choice/modal nodes — see WHITEBOARD_MODEL §2.3.
  Includes a `Native(fn)` escape hatch so no card is ever impossible.
- **Ability kinds:** activated/spell, triggered (+ intervening-if, delayed, state),
  replacement/prevention, continuous/static (→ layers + qualifications).
- **Starter card set** (`cards/`): enough to play real games on a tiny pool — basic
  lands, vanilla/French-vanilla creatures, a handful of removal/burn/draw/counter spells,
  one or two simple enchantments/artifacts. Hand-authored in the IR.
- **Card-data acquisition (later):** two complementary sources —
  1. **Forge `cardsfolder`** (`../forge-ai/forge-gui/res/cardsfolder/`): ~tens of
     thousands of cards already encoded declaratively; a translator from Forge's script
     vocabulary → our IR would bootstrap a large pool. Highest-leverage import.
  2. **MTGJSON / Scryfall** for oracle text, types, mana costs, P/T, set legality — the
     factual layer; pair with a future oracle-text→IR compiler (the GRP analog).

## 6. The decision / agent boundary (the "easy switch")

The single seam through which *all* decisions flow. The engine, whenever a player must
choose, builds a fully-specified, legal-option-masked request; a backend answers.

```rust
pub trait Agent {
    /// Engine asks `player` to decide. `req` enumerates the legal options + constraints.
    fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse;
}

pub enum DecisionRequest {
    PriorityAction { options: Vec<GameAction> },     // cast/activate/pass/special action
    Mulligan       { hand: Vec<ObjId>, to_bottom: u32 },
    DeclareAttackers { eligible: Vec<ObjId>, defenders: Vec<Target> },
    DeclareBlockers  { eligible: Vec<ObjId>, attackers: Vec<ObjId> },
    ChooseTargets  { spec: TargetSpec, legal: Vec<Target> },
    OrderTriggers  { triggers: Vec<StackId> },
    AssignCombatDamage { attacker: ObjId, order: Vec<ObjId>, total: u32 },
    PayCost        { options: Vec<PaymentOption> },
    ChooseModes    { modes: Vec<ModeSpec>, min: u32, max: u32 },
    ChooseNumber   { min: u32, max: u32, forbidden: Vec<u32> },  // X w/ forbidden values
    Distribute     { among: Vec<Target>, total: u32, min_each: u32 },
    SelectCards    { from: Vec<ObjId>, min: u32, max: u32, filter: CardFilter },
    YesNo          { prompt: ChoiceKind },
    // … mirrors MTGA's GREMessageType request catalog (see DECOMPILE_PLAN.md)
}
```

Design constraints that make backends interchangeable:
- **`PlayerView`, not `GameState`.** The agent sees an information-filtered view (hidden
  zones masked per player) — required for both rules-correct hidden info and RL.
- **The request set is the union/superset of** Forge's `PlayerController` decision
  methods (the proven granularity) **and** MTGA's GRE request types (`MulliganReq`,
  `SelectTargetsReq`, `DeclareAttackers/BlockersReq`, `OrderReq`, `PayCostsReq`,
  `AssignDamageReq`, `DistributionReq`, `NumericInputReq`, …). Keeping the enum aligned to
  both means: (a) the Python RL agent and a scripted AI implement `Agent` directly; (b) a
  future `MtgaClientAgent` translates `DecisionRequest`↔ GRE protobuf — an implementation
  swap, no engine changes. See DECOMPILE_PLAN §"mapping to the engine's agent interface".
- **Legal-action masking is the engine's job**, always provided in the request → RL gets
  action masks for free; scripted/human agents can't make illegal moves.

Backends to implement, in order: `RandomAgent` → `ScriptedAgent` (heuristics) →
`PyAgent` (PyO3 → Python policy, see GYM_PLAN) → `MtgaClientAgent` (much later).

## 7. Determinism, snapshots, hidden information (for RL)

- **Seedable RNG** (`rng.rs`); all shuffles/coin-flips draw from it; same seed + same
  agent decisions ⇒ identical game ⇒ replayable.
- **Cheap clone/snapshot** of `GameState` for search (MCTS) and for env `reset`/branching.
  Favor index/`ObjId`-keyed arenas over `Rc`/pointer graphs; aim for `Clone` that's a
  few `Vec` copies, not a deep pointer chase. Consider structural sharing later if needed.
- **Per-player views** computed from full state by a masking function (the only correct
  place to enforce hidden info).
- **Serialization** of full state + decision log for debugging and differential tests.

## 8. Testing strategy

- **Unit tests** per subsystem against specific CR rules (cite the rule number).
- **Scenario tests**: declarative board setups → apply actions → assert resulting state
  (an internal analog of MTGA's scripted regression games / ASUP debug cards).
- **Differential testing vs Forge** (`../forge-ai`): Forge runs full MTG games and has
  a headless sim mode (`run-forge-headless.sh sim …`). Use it as a rules **oracle** — run
  the same deterministic game/seed through both and diff observable state transitions to
  catch rules bugs. Highest-value correctness tool we have for free.
- **Property tests**: invariants (life/zone-count conservation, priority always returns,
  agenda loop terminates, layer recompute is order-independent given timestamps).
- The spec is `docs/rules/comprules.txt` (grep by rule number) + RULES_SUMMARY.md.

## 9. Scope: paper CR vs. "Arena profile"

Target the **paper Comprehensive Rules** as the source of truth, but allow an *Arena
behavior profile* for places MTGA differs (London mulligan default, best-of-1/3, auto-
pass/stops behavior, the exact decision points the client surfaces). The DECOMPILE_PLAN
output tells us precisely which decision points Arena exposes and how it phrases them —
feed that back into the `DecisionRequest` set and the Arena profile.

## 10. Milestones (each ends at something runnable & tested)

1. **Workspace + headless core skeleton.** Cargo workspace; `mtg-core` with state/zones/
   ids/RNG; move GUI out; `RandomAgent`; a CLI that plays a trivial scripted game. Commit.
2. **Turn engine + priority + stack + agenda loop.** No cards yet beyond lands; players
   pass priority through a full turn correctly; SBA loop + trigger ordering wired.
3. **Mana + casting + a vanilla-creature combat game.** Basic lands tap for mana; cast
   vanilla creatures; full combat with damage; lethal/SBA ends the game. First real games
   of self-play between two `RandomAgent`s.
4. **Effect IR v1 + whiteboard commit + replacement pass.** Burn/removal/draw/counter
   spells; ETB triggers; a couple of qualifications (flying, indestructible). Exercise the
   rewrite pass with prevention/"can't" on a real card.
5. **Layer system v1.** P/T pumps, type/color/keyword-granting auras & static effects;
   recompute-on-dirty discipline; tests for timestamp ordering.
6. **PyO3 bindings + Gymnasium env** (handoff to GYM_PLAN): random/PPO self-play on the
   tiny pool with action masking.
7. **Differential testing vs Forge** stood up; bug-fix loop on rules correctness.
8. **Card pool growth**: Forge `cardsfolder` → IR translator spike; expand the playable
   pool toward a real limited/constructed format.
9. **Arena profile + (optional) `MtgaClientAgent`** once DECOMPILE_PLAN delivers the schema.

## 11. Parallel workstreams (for multi-agent execution)

Largely independent once milestone 1 lands; good for fan-out:
- **A. Core engine** (turn/priority/stack/agenda) — milestones 2–3.
- **B. Effect IR + whiteboard + replacement/triggers** — milestones 4.
- **C. Layer/characteristics system** — milestone 5 (self-contained, testable in isolation).
- **D. Decision boundary + agent backends** — milestone 6 entry; coordinate the
  `DecisionRequest` enum with DECOMPILE_PLAN owner.
- **E. Python bindings + gym** — GYM_PLAN, after the boundary stabilizes.
- **F. Card data + Forge import** — milestone 8, parallelizable early as a research spike.
- **G. Differential-test harness vs Forge** — milestone 7, can start once games run.
