# Project State

Single source of truth for goals + where things stand. Update this (without being asked)
whenever meaningful progress changes the picture. Companion: `WORKLOG.md` (chronological).

_Last updated: 2026-06-13_

## Vision (one sentence)

A fast, correct, headless Rust implementation of the MTG rules engine — modeled on MTG
Arena's "whiteboard" architecture — that drives an efficient Gymnasium environment for
training an MTG AI in Python/PyTorch via self-play, with a pluggable decision boundary so
the same engine can be driven by a Python RL agent, a human via a web client, or the real
MTGA client.

## Long-term goals

1. **Full ruleset** implementation (card-agnostic core + a large card pool as Effect-IR data).
2. **Efficient Gymnasium env** (PyO3+maturin) for PyTorch RL self-play at high throughput.
3. **Pluggable decision boundary** — scripted AI, Python RL agent, web-client human, and the
   real MTGA client are interchangeable `Agent` implementations ("the easy switch").
4. **Web play interface + GRE-protocol server** — a from-scratch web client to play against
   mtg-core, built so closely to MTGA's client↔server interface that the real MTGA client
   can be dropped in against our backend (endpoint redirect, or decompile/recompile). The
   recovered GRE protocol is the client seam; see `docs/plans/CLIENT_PLAN.md`.
5. **Expressive enough for MTGA-grade complex cards** (Zurgo/Sylvan Library class) even
   though they're not implemented near-term — the architecture must not foreclose them.

## Short-term goals (next)

1. ✅ **Cargo workspace** + headless `mtg-core` skeleton; GUI out of core (milestone 1, done).
2. ✅ **Turn engine + priority + stack + agenda loop** on a lands-only game (milestone 2, done
   — `state/`/`turn/`/`stack.rs`/`sba.rs`/`priority.rs`; `mtg-cli` self-play harness).
3. ✅ **Mana + casting + vanilla-creature combat** (milestone 3, done) — `cards/` starter
   set + `CardDb`; `mana.rs` (auto-tap payment); casting wired into the `Priority` decision;
   `whiteboard.rs` effect interpreter; `combat/` attack/block/damage; creature-death SBAs.
   `RandomAgent`s self-play to a life-total win; `mtg-cli` demo.
4. Spike the **MTGA decompile** in `../mtga-re` to recover the GRE protobuf schema
   (DECOMPILE_PLAN) — informs the `DecisionRequest` enum.
5. ✅ **Whiteboard rewrite pass + triggered abilities** (milestone 4) — prototype-validated
   (Visionary/Flametongue Kavu/Servant/Fog Bank) AND generalized: GLOBAL-scope replacements
   (Root Maze, Hardened Scales) via `CardFilter::ItSelf`/`ControlledBy`, CR 616.1f player
   choice (`ChooseReplacement`), and dies/LTB triggers (Exultant Cultist).
6. ✅ **Layer system (CR 613)** (milestone 5) — `chars/` computes base ⊕ layered static
   effects with timestamps + dirty→recompute cache; validated on layers 6 (keyword grants),
   7b (set base), 7c (anthems + counters): Glorious Anthem, Levitation (flying → combat
   evasion), Humility. Integrated into SBA/combat/view. Also shipped: Arena-profile auto-pass
   + MTGA stops (#12). Layers 1–5 (copy/control/text/type/color) + CDAs + a genuine 613.8
   dependency case are framework-present/deferred.

## Current state

- **Phase: parallel build kicked off** via a tmux agent team (`mtgenv`, lead + 4 teammates).
  Active workstreams (shared task board): **engine** scaffolding the Cargo workspace +
  headless `mtg-core` (#1); **decompile** recovering the GRE schema **and transport** in
  `../mtga-re` (#2); **design** authoring `AGENT_INTERFACE.md` then implementing
  agent/effects (#3,#4); **client** planning the web client + GRE server (#5).
  **design done with #3:** `docs/design/AGENT_INTERFACE.md` specifies the boundary (one
  `Agent` trait, superset `DecisionRequest`/`Response`, `PlayerView`, Effect IR + `Native`);
  #4 (implement it in `mtg-core`) waits on the scaffold (#1).
  **client done with #5:** `docs/plans/CLIENT_PLAN.md` plans the web UI + `mtg-gre-server`
  (axum+WS, depends only on `mtg-core`); a human is just a `GreSessionAgent` behind the one
  boundary; mapping reconciled to AGENT_INTERFACE §6.1/§1.1; real-client drop-in via
  endpoint-redirect or Mono patch. Transport/auth details blocked on decompile (#2).
- **engine done with #12 + #13 + #14 (Arena stops, layer system, breadth).** **#12:** MTGA-style
  auto-pass / stops (`Engine`'s per-seat `StopConfig`, decision elision) per decompile's
  `priority_stops.md`; plus a live `Engine::stops_handle(p) -> Arc<Mutex<StopConfig>>` so a UI can
  toggle stops mid-game (auto_pass now per-seat). **#13:** the CR 613 layer system (`chars/`) —
  7 layers + timestamps, "affects reads computed-prior-layer types" (613.8), over design's
  `StaticContribution`; validated on anthems / Levitation / Humility / Nature's Revolt. **#14
  (engine breadth, DONE):** evergreen keywords (flying/reach, first/double strike with the combat
  two-substep, trample, deathtouch, lifelink, vigilance, menace, defender, haste, flash, hexproof,
  indestructible); **auras + equipment** (attachment subsystem: `attached_to`, `CardFilter::
  AttachedHost`, Aura enters-attached + fall-off SBA, Equipment + the activated-ability path +
  unattach SBA; a qualification dimension on `ComputedChars` read by combat); **planeswalkers**
  (printed loyalty, enters-with-loyalty, loyalty abilities once/turn at sorcery speed via
  `CostComponent::Loyalty`, attackable + combat damage removes loyalty, 704.5i 0-loyalty SBA).
  ~38-card starter set; 84 mtg-core tests green. Deferred: ward/shroud, layers 1–3, planeswalker
  ultimates, general enchant restrictions.
- **engine: milestone 4 prototype validated (#11).** The whiteboard model holds up against
  the CR on concrete cards: triggered abilities (events → APNAP stack → resolve, with
  trigger targeting) and a real materialize→rewrite→commit pass (replacement/prevention) wired
  over design's `ActionPattern`/`Rewrite`. ETB, spell damage, and combat damage all flow
  through the whiteboard. Validated cards: Elvish Visionary, Flametongue Kavu, Servant of the
  Scale (enters-with-counter), Fog Bank (prevent combat damage). **Now generalized:** GLOBAL
  replacements scanned across the battlefield (Root Maze, Hardened Scales) via
  `CardFilter::ItSelf`/`ControlledBy`, CR 616.1f player choice (`ChooseReplacement`), and
  dies/LTB triggers (Exultant Cultist). Next: the layer system (M5), more event/action
  patterns, regeneration/delayed triggers as cards need them.
- **engine done with #1 + #7 + #9 (milestones 1–3):** the headless `mtg-core` now runs a
  **minimal but real game** — turn machine (CR 500s), stack (CR 405), priority loop + agenda
  fixpoint (CR 117.5/603.3/704.3), mana + casting (CR 601) with auto-tap, an Effect-IR
  interpreter (whiteboard.rs: DealDamage/Draw/GainLife), vanilla-creature combat (CR 506–511),
  and creature-death SBAs (704.5f/g). A `cards/` starter set (lands, 2 creatures, Shock,
  Divination, Healing Salve) + R/G demo deck; `CardDb` rides in `GameState` as `Arc`
  (serde-skipped). Two `RandomAgent`s self-play to a life-total win (`mtg-cli`). Built on
  design's `agent.rs`/`effects/`/`basics.rs`; the interpreter lives in the engine over
  design's IR. Deferred (M4–M5): whiteboard replacement/prevention pass, the layer system,
  keywords, mana-via-IR, the PayCost agent decision, mulligans.
- **webui done with #8 (M1+M2):** `mtg-gre-server` plays a human (browser, seat 0) vs `RandomAgent`
  through the one `Agent` boundary — axum + WebSocket, `GreSessionAgent`, a JSON projection of
  `DecisionRequest`/`Response`/`PlayerView` with the engine's legal-option masking. An MTGO-style
  board: real card frames (Scryfall art + official mana-symbol SVGs, baked manifest — no runtime
  API), hand at the bottom, lands/creatures split, a 12-step phase bar, clickable GY/exile/decklist
  zone viewers, hover→full-card preview, deck picker (Burn/Bears/demo). **MTGA auto-pass/stops are
  engine-owned**: the socket holds the seat's live `Engine::stops_handle` (`Arc<Mutex<StopConfig>>`,
  passed out of the game thread via a oneshot) and toggles it mid-game with no reset — web + CLI now
  share the one engine policy (no duplicated client-side logic). **Stops are per turn-side**: the
  phase bar shows two dots per step (your turn / opponent's turn, independently toggleable, over the
  engine's `(Phase, own_turn)` overrides). **MTGA keybindings**: Space = pass / take the sole forced
  action; Enter = pass through this turn's remaining priority stops (lapses next turn, badge shown);
  Esc cancels.
  **Library peek is RL-safe** — a static starting decklist snapshotted server-side from `GameState`
  (never via `PlayerView`, so it can't leak draws to the RL agent). Ships as a no-build embedded
  client *and* a Vite/TS client (kept in sync). Also an expressive scriptable CLI (`mtg-cli`-style
  scenario setup/inspection). Blocked-on-decompile bits (real GRE transport/auth) remain M3.
- `mtg-core` is headless (no `egui`/`eframe`); the engine is built fresh under the workspace.
- Docs in place: architecture (`docs/design/WHITEBOARD_MODEL.md`), rules
  (`docs/rules/RULES_SUMMARY.md` + PDF/text), plans (`docs/plans/`), `CLAUDE.md`.
- Decompile recon done: MTGA = Mono + protobuf, Steam install; full decompile in progress.

## Key decisions

- **Whiteboard model** as the engine architecture (see WHITEBOARD_MODEL.md).
- **Core is card-agnostic**; card behavior is data (Effect IR) + `Native` escape hatch.
- **Single `Agent`/`DecisionRequest` boundary** with engine-provided legal-action masking;
  ALL front-ends (RL, scripted, web, MTGA client) are backends of this one boundary.
- **The GRE protocol is the client seam.** A GRE-protocol server wraps `mtg-core`; the web
  client and the real MTGA client are both clients of it → decompile must capture transport,
  not just message schemas.
- **PyO3 + maturin** for the RL hot path; a socket transport option for the GRE/web client.
- Target **paper CR** as truth + an **Arena profile** for MTGA-specific behavior.
- Validate rules correctness via CR-derived unit/expect tests + the captured MTGA Detailed-Logs.
  (No Forge — it's the abandoned prior attempt this project replaces, not a reference/oracle.)

## Risks / open questions

- Layer system (CR 613) + replacement-effect interaction are genuine fixpoint computations
  — the hardest correctness surface.
- Action-space design for RL (huge, variable) — factored + masked vocabulary, autoregressive
  later.
- State must stay cheaply cloneable/serializable for MCTS + vectorized envs.
- Card-pool acquisition strategy: MTGJSON/Scryfall oracle data → Effect-IR (oracle-text→IR compiler).
- Legal/ToS care around MTGA decompilation (personal research/interop; don't redistribute
  WotC code/assets) — see DECOMPILE_PLAN.
