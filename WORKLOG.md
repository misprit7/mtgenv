# Work Log

Short, dated entries for future-agent consumption. Newest first. One line or a few bullets
per unit of meaningful progress. Keep it terse — detail lives in `docs/` and git history.

## 2026-06-13

- **engine:** task #12 — **Arena-profile priority auto-pass + MTGA-style stops** (decision
  elision, AGENT_INTERFACE §8.1) layered over the CR-correct priority loop. The engine still
  grants priority at every window; the policy elides the `Priority` prompt (treats it as a
  pass without consulting the agent) per `should_auto_pass`: never auto-passes a stop or under
  full control; with the policy off it always prompts. Rules: auto-pass when no non-pass
  action (except own MP1, a default stop); auto-pass through unimportant steps (upkeep/draw/
  begin+end combat/end) even with actions unless a stop is set; default stops = own MP1/MP2,
  declare-attackers (own turn), declare-blockers (defending). Per-seat `StopConfig`
  (full-control toggle + per-step overrides) on the `Engine`; API: `set_arena_auto_pass`,
  `set_full_control`, `set_stop(p, step, Option<bool>)`, `is_stop`, `stop_config`. **Off by
  default** (paper-CR / deterministic for differential-test + RL replay); a human/UI session
  enables it. Forced choices (targets/order/discard/mulligan/combat declarations) are
  untouched. 49 mtg-core tests green (policy unit tests + an end-to-end spy: minor steps
  elided, full-control prompts everywhere); workspace green, clippy clean. webui pairs the UI
  (stop toggles + full-control + phase/active-stops display).
  - **Refined to decompile's recovered MTGA spec** (../mtga-re/docs/priority_stops.md):
    persistent default stops are now **MP1/MP2 only** (declare-attackers/blockers are forced
    turn-based actions, not priority stops — dropped from defaults vs the task's literal list);
    added **SmartStops** (per-seat, MTGA default ON) = prompt wherever you have a legal play
    (replaces "auto-pass unimportant even with an action"; that's now the SmartStops-OFF mode).
    API adds `set_smart_stops(p, on)`.
  - **stackAutoPassOption = ResolveMyStackEffects** (MTGA default ON, per-seat) now implemented
    (the in-response-to-your-own-spell nuance the user asked about): while your OWN object is on
    top of the stack you auto-pass so it resolves — you're not re-prompted to respond to
    yourself; the opponent is still prompted to respond when they can act; full control / a stop
    override. `set_resolve_own_stack(p, on)`. Also added the MTGA `AutoPassOption` enum
    (UnlessAction/UnlessOpponentAction/ResolveMyStackEffects/ResolveAll/FullControl) +
    `set_auto_pass_option(p, opt)` mapping it onto the seat's flags (vocabulary for the UI; finer
    Turn/EndStep/ResolveAll distinctions approximated, refined later vs byte-exact defaults).
    Deferred: yields/answers, transient stops, captured ConnectResp.settings defaults.
- **engine:** task #11 GENERALIZATION (milestone 4 cont.) — the rewrite pass + triggers are
  now beyond the self-scoped prototype (4 snapshot commits): (1) land plays routed through the
  whiteboard + `Rewrite::EntersTapped`/`Action::TapUntap`; (2) a **dies/LTB trigger** (Exultant
  Cultist "when this dies, draw") via the existing SelfDies path (source found in graveyard by
  grp_id); (3) **GLOBAL-scope replacements** — the pass now scans every battlefield permanent's
  `Ability::Replacement` (not just the affected object's own), with `CardFilter::ItSelf` /
  `ControlledBy(Controller)` evaluated against the replacement's source (design added ItSelf +
  `WouldAddCounters{kind,to}`). Validated on **Root Maze** (global "lands enter tapped" taps an
  opponent's land) and **Hardened Scales** (global "+1/+1 on a creature you control → +1 more"
  modifies Servant of the Scale's own enters-with-a-counter — a replacement modifying another
  replacement's output, resolved by the fixpoint → 0/0 enters as 2/2). Converted Servant/Fog
  Bank from `Any` to `ItSelf` (else they'd leak globally). (4) **CR 616.1f** player choice — when
  >1 replacement applies to one event, the affected object's controller picks via
  `DecisionRequest::ChooseReplacement`, then re-check; validated with two Hardened Scales (1+1+1
  ⇒ 3 counters, decision surfaced). 47 mtg-core tests green, workspace green, clippy clean.
- **engine:** task #11 (ENGINE_PLAN milestone 4) — **prototype-first** validation of the
  two architecture-defining subsystems, on 4 Scryfall-verified cards (4 snapshot commits):
  (1) TRIGGERED ABILITIES (CR 603): commit emits events → `collect_triggers` queues matching
  `Ability::Triggered` → agenda drains APNAP → `put_trigger_on_stack` chooses targets
  (603.3d) → resolve via the interpreter. `StackObjectKind::Ability { index }` carries which
  ability fired (looked up by grp_id, persists across zones). Validated: **Elvish Visionary**
  (ETB draw, non-targeting) + **Flametongue Kavu** (ETB 4 to target creature → lethal SBA).
  (2) WHITEBOARD REWRITE PASS (CR 614/616): real materialize→rewrite→commit replacing the M3
  straight-through, with the once-per-replacement guard + fixpoint, wiring design's
  `ActionPattern`/`Rewrite`. Validated: **Servant of the Scale** (Rewrite::EntersWithCounters —
  a 0/0 enters as 1/1 and survives) + **Fog Bank** (WouldBeDealtDamage{Combat}+Prevent — combat
  damage prevented). ETB + spell damage + combat damage now all flow through the whiteboard.
  Added `Object::effective_power/toughness` (counters affect P/T — trivial layer-7c) so the
  enters-with-counter is observable. Each interaction has an expect-test trace; 43 mtg-core
  tests green, full workspace green, clippy clean. CR/design notes (for generalization): a
  `CardFilter::ItSelf` + global-replacement consultation are needed beyond self-scoped
  replacements; 616.1f player-choice among replacements deferred. Coordinated with design (no
  effects/ change needed).
- **webui:** task #8 follow-ups (interactive play deepened). (1) Swapped the temporary driver
  for engine's real `Engine::run_game` (removed duplicated rules logic). (2) Built an
  **expressive CLI** (`mtg-play`): scenario setup (`new`/`life`/`add`/`deck`/`handsize`/`seat`),
  inspection (`show` god-view / `show <p>` filtered `PlayerView`), and a **scriptable** mode
  (`--script`, deterministic) — `cli.rs`/`render.rs`, shared option projection so CLI + web mask
  identically. (3) `play [decks…] [seed]` deals the engine's real decks — **demo** (creatures+burn)
  default, plus the user's **`play burn bears`** matchup — so casting, targeting (Lightning Bolt),
  combat and the damage/deck-out win conditions all surface (game-over prints the `end_reason`).
  The web board now deals the demo deck too (creatures render in-browser). (4) Wired engine's new
  `skip_opening_deal()` so `deal off` plays a hand-built scenario as-is. expect-test snapshots of
  the CLI render + the JSON wire projection (living protocol docs). 13 crate tests green; full
  workspace green. Next: place named starter-db cards in scenarios (`add … "Grizzly Bears"`).
- **engine:** post-M3 follow-ups (3 small commits): (1) adopted design's canonical
  `basics::CardType`, deleted the `state::CardType` duplicate (one import path); (2) added
  scenario hooks for webui's CLI — `Engine::skip_opening_deal()` (play a hand-built scenario
  with no shuffle/deal), public `Engine::legal_actions(p)` (pre-render the masked option set),
  and an `Outcome { winner, turns, reason }` via a new `GameState.end_reason`; (3) task #10 —
  added **Lightning Bolt** ({R}, 3 to any target) and the **Burn** (40 Bolt + 20 Mountain) /
  **Bears** (40 Grizzly Bears + 20 Forest) preset decks + `preset_deck`/`burn_vs_bears_game`;
  `mtg-cli` now takes deck args (`mtg-cli <seed> burn bears`). 39 mtg-core tests green,
  full workspace green (mtg-gre-server 10), clippy clean.
- **engine:** implemented task #9 (ENGINE_PLAN milestone 3) — a minimal PLAYABLE game:
  mana + casting + vanilla creatures + combat. New `cards/` module: `CardDef`
  (Characteristics + design's `Ability` IR) + a `CardDb` registry keyed by `grp_id`; a
  starter set (4 basic lands, Grizzly Bears 2/2, Hill Giant 3/3, Shock = 2 to any target,
  Divination = draw 2, Healing Salve = gain 3) + an R/G demo deck. `GameState` gains
  `card_db: Arc<CardDb>` (serde-skipped — card *data* out of snapshot state) + a `combat`
  field. `mana.rs`: mana sources, affordability, engine auto-tap payment (CR 605/118).
  Casting (CR 601, in `priority.rs`): `Cast` wired into the `Priority` decision with
  sorcery-vs-instant timing, target choice (601.2c), auto-pay, the stack; resolution runs
  the effect IR. `whiteboard.rs`: the **effect interpreter** over design's `Effect`
  (DealDamage/Draw/GainLife/LoseLife/Sequence) → materialize `Action`s → commit + emit
  events (replacement pass deferred to M4). `combat/`: declare attackers/blockers, combat
  damage (single/multi-block w/ `AssignCombatDamage`), simultaneous dealing; `sba.rs` adds
  creature death (704.5f/g). Two `RandomAgent`s now play lands→creatures→attack→damage to
  0 life (mtg-cli demo). 35 mtg-core/cli tests green incl. expect-test snapshots (cast
  Shock, unblocked attack, blocker trade, a full R/G combat-trace game); `cargo build`/
  `test`/`clippy` clean for mtg-core+mtg-cli. Coordinated the interpreter boundary with
  design (engine owns the interpreter over their IR); added `pub mod cards;` to lib.rs.
  Flagged a `CardType` duplication (mine in `state` vs design's in `effects::target`) for
  consolidation into `basics`. Deferred (M4+): keywords, layers, replacement/prevention,
  mana-via-IR, PayCost agent decision (auto-tap for now), mulligans.
- **webui:** implemented task #8 (CLIENT_PLAN M1–M2). New crate `crates/mtg-gre-server`
  (depends only on `mtg-core`): `human.rs` = **M1** stdio `HumanAgent` (a human is just another
  `Agent`); `session.rs` = **M2** `GreSessionAgent` bridging the boundary over a WebSocket via a
  **JSON projection** (`protocol.rs`); `options.rs` = shared request→`Prompt`→response
  projection so CLI + web render the *same* engine-enumerated legal set (masking); `server.rs`
  = axum host (`/ws` + static `web/dist`, with a no-build embedded client fallback). TS/Vite
  front end under `web/` (board/hand/stack + legal-only affordances). A **temporary** lands-only
  `driver.rs` runs the boundary until engine's loop is wired in (it uses only `mtg-core`'s
  public API). Verified: CLI plays a full game (`--bin mtg-play`); browser plays a full game vs
  `RandomAgent` (`--bin mtg-serve`, both embedded + Vite builds, screenshot-checked); `cargo
  build`/`test` green. TODO: swap `driver.rs` for engine #7's `Engine` entry point.
- **engine:** implemented task #7 (ENGINE_PLAN milestone 2) — a runnable lands-only game
  loop. New code in `mtg-core`: `state/` (`GameState`/`Player`/`Object`/`Characteristics`/
  `CardType`, `ObjId`-keyed arena, zones as `ObjId` vecs, `move_object`/`draw`/`shuffle`;
  `state/view.rs` = the `view_for(seat)` hidden-info masking that builds design's
  `PlayerView`), `turn/` (the CR-500s 12-step sequence + `step_grants_priority`/
  `is_main_phase`), `stack.rs` (the LIFO stack + `StackObject`), `sba.rs` (the player-loss
  SBAs 704.5a–c, esp. decking 704.5b), and `priority.rs` (the `Engine`: turn driver,
  turn-based actions, the **priority loop** with hold-priority/APNAP pass counting, and the
  **agenda fixpoint** recompute→SBA(loop)→triggers(APNAP)→priority per WHITEBOARD_MODEL §2.2).
  Choices flow through design's `Agent` trait (`RandomAgent`); only legal action in M2 is
  play-a-land (CR 116.2a), engine-masked. `mtg-cli` is now a lands-only self-play harness
  (`mtg-cli [seed] [lib]`) — two `RandomAgent`s deck each other out with no panics. Added
  `serde` to `Rng` so `GameState` snapshots/replays. 26 tests green incl. expect-test
  snapshots (enumerated legal options at a decision point; the one-turn CR-500s trace);
  `cargo build`/`test`/`clippy` all clean. Did NOT touch design-owned files
  (`agent.rs`/`effects/`/`basics.rs`/`error.rs`); no `lib.rs` change needed (filled existing
  module stubs). Deferred to M3+: mana/casting/combat declarations, the new-object rule on
  zone change (400.7, irrelevant lands-only), mulligans.
- **design:** implemented task #4 — the agent boundary + Effect IR are now real code in
  `mtg-core` (commit 360d3a6). New: `agent.rs` (the `Agent` trait, `DecisionRequest` 21-variant
  enum, `DecisionResponse`, `PlayerView` + view types, all supporting request types, `GameEvent`,
  and a `RandomAgent` reference backend that can only pick legal options); `effects/` split into
  `mod.rs` (the `Effect` IR), `action.rs` (`Action`/`Whiteboard`), `ability.rs` (the 5 ability
  kinds + costs/keywords/qualifications), `value.rs`/`target.rs`/`condition.rs`/`native.rs`
  (the `Native` escape hatch). Plus shared `basics.rs` (Color/Zone/Phase/Status/ManaCost/
  ManaPool/CounterKind/CounterBag/DamageKind/Target/ZoneDest — one canonical home; **engine
  imports these, doesn't redefine**) and `error.rs` (EngineError). `cargo build`+`cargo test`+
  `cargo clippy` all green; 6 unit tests (RandomAgent legality, ChooseNumber constraint
  honoring, determinism-by-seed, serde round-trip). Boundary types derive serde (the §1.1
  GRE-server contract). One open item flagged: batched `CastingTimeOptions` needs a multi-part
  response (decompose vs. structured) — ratify with engine/gym/client at integration.
- **design:** reconciled `AGENT_INTERFACE.md` against the recovered+log-validated GRE schema
  (decompile's `../mtga-re/`) — §9 now RESOLVED, not open. Confirmed strict-superset holds
  (variant set unchanged); enriched `ChooseNumber` to match `NumericInputReq` exactly
  (`step`/`disallow_even`/`disallow_odd`; `forbidden`↔`disallowedValues`). Key validation:
  GRE `CastingTimeOptionReq` embeds `numericInputReq`/`modalReq`/`selectNReq` as inner
  messages — i.e. GRE's own wire literally decomposes a cast's options into our
  ChooseNumber/ChooseModes/SelectCards sub-steps. `TargetSelection` ≅ our `TargetSlot`.
  Also added §8.1: decision *elision* (auto-pass / forced-single-option) is an engine/Arena-
  profile concern, uniform across all backends (load-bearing for differential-testing/replay).
- **client:** wrote `docs/plans/CLIENT_PLAN.md` (task #5) — web play UI + a **GRE-protocol
  server** (`mtg-gre-server` crate, axum + WebSocket, depends only on `mtg-core`) fronting the
  engine. A human at the web UI is just another `Agent` backend (`GreSessionAgent`) — same
  single boundary as RL/Gym and scripted AI. The seam is the GRE protocol itself, so the
  **real MTGA client can be dropped in** (two strategies: protocol-compatible server +
  endpoint redirect, vs. patch/runtime-hook the Mono client). Milestones: CLI text client →
  minimal web board (JSON) → protocol-compatible server (recovered protobuf) → real-client
  drop-in. Reconciled the DecisionRequest⇄GRE mapping to `AGENT_INTERFACE.md` §6.1; the docs
  now cross-reference (design added §1.1 GRE-server serialization contract).
- **client (follow-up):** decompile #2 landed → folded the **recovered + log-validated GRE
  transport + schema** into CLIENT_PLAN §4/§5/§8 (no longer assumptions): wire = TLS 1.2 over
  TCP, custom **6-byte frame** `[ver=4][type|format][int32 LE len]` inside the TLS stream +
  ping/pong keepalive; envelope = `IMessageEnvelope{Protobuf|Json, Compressed, TransId}` w/
  protobuf payload as `Any`; **endpoint is dynamic** (match push `MatchInfoV3.MatchEndpointHost/
  Port`+`MatchId`); GRE `ConnectReq` is **tokenless** (auth binds upstream). Net: real-client
  drop-in **de-risked** — no GRE token to forge, TLS solvable via controlling the pushed
  hostname + local dev-CA (no pinning bypass). Mapping table updated to exact recovered resp
  names; sent transport facts to decompile for their #6.
- **design:** wrote `docs/design/AGENT_INTERFACE.md` — the single `Agent` trait +
  `DecisionRequest`/`DecisionResponse` enums + `PlayerView` (info-filtered, hidden zones
  masked) + the Effect IR / whiteboard `Action` / `Native` hatch (Rust sketches). The
  `DecisionRequest` set is a proven **superset** of the recovered MTGA GRE `*Req` catalog
  (coverage matrices in §6). Masking is the
  engine's job. Asked `decompile` for field-level GRE Req/Resp shapes (§9 open questions);
  variant set not expected to change. Task #4 (implement agent.rs + effects/) blocked on
  the workspace scaffold (#1).
- **Project bootstrapped into a planned project.** Established docs, the architecture, and
  the implementation plans.
- Downloaded the MTG Comprehensive Rules (eff. 2026-02-27) → `docs/rules/`
  (`MagicCompRules_20260227.pdf` + extracted `comprules.txt`).
- Wrote `docs/rules/RULES_SUMMARY.md` — engine-implementer's map of the CR (layers, SBAs,
  priority/stack, combat, replacement/triggers, keyword index), with rule numbers.
- **Architecture decided: the MTGA "whiteboard" model** (per WotC dev diaries) →
  `docs/design/WHITEBOARD_MODEL.md`. Card-agnostic core + declarative effect rules that
  rewrite a pending-actions whiteboard; agenda pipeline; qualifications; layers; LKI.
- Wrote `docs/plans/ENGINE_PLAN.md` (Rust workspace, milestones, agent boundary, testing
  CR-derived expect tests + MTGA logs), `docs/plans/GYM_PLAN.md` (PyO3+maturin, action masking,
  self-play), `docs/plans/DECOMPILE_PLAN.md` (MTGA protocol recovery).
- Recon: **MTGA is a Mono build** (not IL2CPP), Steam install, **protobuf** GRE protocol
  (`Wizards.MDN.GreProtobuf.dll`). Decompile is the easy path; work to live in `../mtga-re`.
- Wrote `CLAUDE.md` (orientation + conventions) and these trackers. Initialized git history.
