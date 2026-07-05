# Project State

Single source of truth for goals + where things stand. Update this (without being asked)
whenever meaningful progress changes the picture. Companion: `WORKLOG.md` (chronological).

_Last updated: 2026-07-04_

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

- **✅ M3 RESUMABLE ENGINE (2026-07-03): engine side complete, port live at 1.5–1.67×.**
  `Session { resume/submit/replay }` (corosensei fiber yields at the single `ask` seam; game logic
  byte-identical; ~390 tests green). mtg-py drives games through it — per-game threads/channels/
  PyAgent DELETED, decision trajectories byte-for-byte identical to the old transport (committed
  fingerprint gate). Send-split priced and REJECTED (Deref two-phase dealbreaker; Agent:Send =
  LAW change) → **thread-pinned fleet groups** (zero engine change, nothing Send). **M3 CLOSED
  2026-07-03: FleetSelfPlayVecEnv shipped — end-to-end training ~1.6–2.0k (pump) → 2.25k (Session)
  → 5.5k fps (fleet, 512 envs) = 2.8×**, byte-identical trajectories + same learned policy at every
  step; fleet is the default vec env. Deferred (resume when throughput binds): pump-loop
  vectorization toward fully-GPU-bound. Design + fork record: docs/design/RESUMABLE_ENGINE.md.
- **✅ TRAINING VERIFIED WORKING (2026-07-03) + shaping default ON.** The "heralds" sanity deck
  (40× Mist-Cloaked Herald + 20× Island, provably-optimal play = land/cast/attack-all) trains to
  greedy attack_rate 1.000, productive_rate 1.000, 0.972 vs random (baseline 0.478). PBRS shaping
  (0.5·tanh(Δlife/10)+0.3·tanh(Δpower/6)+0.2·tanh(Δcards/4), 60% anneal) now defaults to coef 0.5;
  eval stays raw ±1. Per-window cast/playland_rate cap <1.0 for optimal play (mutual-exclusion
  artifact) — use `productive_rate` as the convergence gauge. TB served from /tmp/mtgenv_tb/.
- **✅ MOBILE WEB CLIENT (2026-07-03).** Game client + lobby fully playable from a phone: mobile
  reflow (sticky prompt sheet, opp-top/you-bottom strips, log toggle), touch previews long-press-only
  (hover gated to real mice), on-screen pass-turn button. Desktop unchanged.
- **▶ SOS FULL-SET scope (2026-07-04, user directive — T4 deferral revoked): ~161 authored, 595 mtg-core
  tests green (+ registered the missing Swamp basic land).** sos-cards-9 shipped 4 caps + 3 cards: S12
  target-dependent cost reduction (Ajani's Response), enters-tapped MoveZone (Teacher's Pest), Exile-as-cost
  (Postmortem Professor) — the graveyard-recursion trio is complete. Each subsystem built as the general CR capability, not the minimal hack (the big three still
  ahead: Lessons/Learn, Planeswalkers, prepare-DFCs). **sos-cards-9 finished the S12 target-dependent
  cost-reduction sub-cap** (the piece agent-8 deferred as risky): `CostReductionCondition::{State|TargetMatches}`
  + `effective_cast_cost(TargetCtx::{Optimistic|Chosen})`; `cast_spell` recomputes cost from chosen targets and
  constrains target candidates to what the caster can pay — no rewind. → Ajani's Response.
- **🎯 SOS FIRST-PASS MILESTONE (2026-07-03 night): 153 authored / 150 fully-faithful / 3 tracked-partial, 575 tests.** (Declared at 147/144/558; +2 cheap-vein sweep — Pensive Professor, Potioner's Trove; +4 moderate-cap cards sos-cards-7 — Berta, Growth Curve, Living History, Emil.) Of 271 distinct SOS cards: 153 done, 36 prepare-DFCs deferred (first-pass scope), 7 deferred-by-type (planeswalkers/Lessons); the cheap vein is SWEPT and the moderate-cap tier is being worked (each remaining unauthored card needs a genuinely-new cap), with ~60 behind bigger subsystems (spell-copy deferred: ~1 net card ROI; Fractalize = milestone-5 layers). Seven-agent relay chain, one continuous ledger (docs/plans/SOS_CARDS.md), 53+ engine caps built, real-path bugs found+fixed along the way. **sos-cards-7 shipped 5 caps + 4 cards**: {X}-in-activated-cost → Berta; CountersOnTarget+flush-before-PutCounters → Growth Curve; CardFilter::Attacking → Living History; DistinctNames value + HasCounter-in-static-scope → Emil.
- **▶ SOS card push (long-term, active):** Secrets of Strixhaven 271 cards triaged in
  `docs/plans/SOS_CARDS.md` — 74 authorable now, 142 behind ~small caps (7 caps unlock ~79),
  55 deferred (MDFCs + big subsystems). Agent grinding easiest-first. Latest: **S15 impulse-play DONE for
  exile cases** (`d079eb0` base [adopted from an orphaned predecessor WIP, hardened + tested] + `0e17d3e`
  top-of-library source + land-play-from-exile) → Practiced Scrollsmith, Elemental Mascot, Suspend
  Aggression. Then **S13 restricted-mana DONE** (`ffcc0df`, `ManaSpec.restriction` + `ManaPool.restricted`
  bucket, `allow_restricted` threaded through payment) → Hydro-Channeler. Then **Select-exile-as-cost**
  (`5596fb4`, `Effect::Exile` handles a resolution-time `Select`, gates `IfYouDo`) → Heated Argument. 480
  mtg-core tests green. Then **begin-of-step-trigger cap** (`20965a8`): `collect_triggers` now queues
  `BeginningOfStep(phase)` permanent triggers + evaluates `Triggered.condition` (CR 603.2/603.4), fixing 4
  latent-partial cards (Startled/Essenceknit/Primary Research/Additive Evolution) with turn-engine
  integration tests. Then **Abstract Paintmage** (`00e18a9`, first-main trigger floats restricted {U}{R} —
  exercises both new caps end-to-end), plus **HasKeyword** (Glorious Decay), **Multicolored** (Mage Tower
  Referee), and **multi-player ForEach** (Splatter Technique) filter/area caps. Proposed a "every trigger
  fires through the real engine" audit rule. **Session (agent sos-cards-3, 2026-07-03): 9 cards + 7 caps,
  ~117→~126 SOS, 496 mtg-core tests green.** Handed off at context-fatigue with a prioritized next-steps
  block at the TOP of SOS_CARDS.md (multi-target MoveZone is the highest-yield next cap → 3 cards).
  **Session sos-cards-4 (2026-07-03): 5 cards + 4 caps** (multi-target MoveZone, source-threaded `Not(ItSelf)`,
  S21 cast-with-{X}, CreateToken dynamic counters) → 509 tests. **Session sos-cards-5 (2026-07-03): S17 Ward
  cap (mana + discard) + 2 cards** — `Effect::CounterUnlessPay` soft-counter + `EffectTarget::Triggering`
  (targeting spell threaded through `GameEvent::Targeted.source` → `ResolutionCtx.triggering_stack`) →
  **Colorstorm Stallion** (Ward {1}, `96dbc35`) + **Forum Necroscribe** (Ward—Discard, `c335bcd`) + **Tragedy
  Feaster** (Ward—Discard + Infusion end-step sacrifice, `1ca6d8e`); also fixed `Effect::MoveZone` target
  collection. Then a **ledger-vs-git audit** flipped 4 more stale ⏳ rows (S2/S3/S18/S11 all already done) +
  added a same-commit process rule; and **Antiquities on the Loose** (`8ed83b1`, S10 flashback front-cap:
  `Condition::CastFromNotHand` + a #61 `CreateToken`-commit-before-next fix). Then an Explore-subagent
  unauthored-card audit refreshed the queue and I swept **4 more no-cap cards** (Rancorous Archaic, Aberrant
  Manawurm, Topiary Lecturer, and **Thornfist Striker** = the 4th Ward card via `ConditionalStatic`). Then,
  after confirming **lifelink is already combat-wired** (corrected a wrong belief), **Inkshape Demonstrator**
  (5th Ward card) + **Hardened Academic**. This session (sos-cards-5): **11 cards + S17 Ward cap +
  CastFromNotHand cap + 2 engine fixes (MoveZone target collection, CreateToken commit-ordering) + a full
  ledger-vs-git cap audit.** Ward: 5 of 8 done (mana + discard). → **536 mtg-core tests green.**
  **Session sos-cards-6 (2026-07-03): per-turn "counter put on this permanent" cap + Fractal Tender (6th Ward
  card) → 541 tests.** New `Object.counter_added_this_turn` (set in the `AddCounters` executor) +
  `Condition::PutCounterOnSelfThisTurn`. ALSO corrected two wrong "unwired" beliefs by reading the code:
  first/double-strike combat damage is **already wired** (CR 510.4 two-substep in `combat/mod.rs` since `a15015f`,
  with passing tests) — the handoff's #1 task was a no-op. Then **Homesickness + `Effect::ForEachTarget` cap**
  (apply a body to each chosen target of a variable multi-target slot, reusing `EffectTarget::Each`) → 545 tests.
  Then **Fractal Anomaly + S19 `ValueExpr::CardsDrawnThisTurn`** (per-player cards-drawn counter, reset each turn) →
  548 tests. Session sos-cards-6 total: **3 caps + 3 cards** (Fractal Tender, Homesickness, Fractal Anomaly).
- **✅ #60 END-TO-END AUDIT COMPLETE — all 18 cards driven through the REAL cast→pay→resolve loop.** The
  prior behaviour tests called `resolve_effect` directly, bypassing casting + mana payment, so "18/18
  fully implemented" was *asserted, not proven*. This audit rebuilt a harness on the engine's `pub(crate)`
  seam (`cast_spell`/`play_land`/`activate_ability`/`resolve_top` + `run_agenda` + `declare_attackers_explicit`
  + `legal_actions`) and drove every card through real auto-paid mana, targeting, modes, costs, and
  ETB/landfall/attack/becomes-targeted triggers — asserting every oracle clause against resolved state.
  **Result: 18/18 confirmed; re-baseline changed NO flag** (the 17 `true` are now *validated*; Surrak stays
  `false`). **Bugs found: 1 — #64 (FIXED):** Keen-Eyed's graveyard-targeting exile silently fizzled because
  `target_legal` only accepted battlefield targets (the resolve-level test masked it by bypassing
  `resolve_top`'s guard); engine made the re-check spec-aware. Verified the user-found mana fixes (#56/#57
  via #59) end-to-end too. Full per-clause matrix + report in `docs/plans/SELESNYA_LANDFALL_CARDS.md`.
  **231 mtg-core lib tests green; clippy clean.**
- **🎉 Selesnya Landfall push DELIVERED — #44 COMPLETE.** All **18 distinct cards authored**; the
  `selesnya`/`landfall` preset (`cards::selesnya_landfall_deck`) **is the real mtggoldfish 60** (51 nonbasics
  + 7 Forest / 2 Plains, no padding) and plays end-to-end (validated via `mtg-cli` self-play across many
  seeds, clean finishes, zero panics; the deck is also gym's live training pool). **17/18 cards fully
  faithful; the deck is 18/18 fully-faithful minus only Surrak's inert "can't be countered"** — every
  substantial clause on every card is implemented; Surrak's lone gap (a `CantBeCountered` qualification with
  no counterspell in the pool to act on) is a documented standing deferral per the lead. **Behaviour-test
  coverage: 16/18 cards have a card-module resolve-level test** (zones/counters/P-T/mana, not just IR);
  Temple Garden (ETB replacement) + Surrak (becomes-targeted event) have no standalone resolvable effect and
  are covered by engine cap tests (C11/C16) + IR snapshots. **197 mtg-core tests green; clippy clean.** Engine
  caps **C1–C20 all landed** (incl. C13 Crew + C18 land-permissions); plus many ad-hoc subsystems (floating
  continuous-effects, delayed triggers, warp, becomes-targeted + stack-half, exile-association, snapshot pump,
  Optional/ForEach, reflexive sub-trigger, …). The
  authoritative **capability ledger** (every cap ✅/⏳ + the card it enables + commit refs) lives in
  `docs/plans/SELESNYA_LANDFALL_CARDS.md`. Remaining upgrade-tail caps (each flips one ⚠ card to fully
  faithful, no new deck cards): reflexive sub-trigger (Earthbender/Dyadrine), distinct two-target removal
  (Dyadrine), Crew (Lumbering), searched-ref+Untap (Fabled), CantBeBlocked (Escape), Target::Stack (Surrak),
  C18 (Icetill), reflexive-mana (Badgermole). 169 mtg-core tests green.
- **engine: C15 (double-power) + C16 (becomes-targeted) landed + #43 (search-reveal).** `PumpPT` is
  now materialized as a floating `ModifyPT` continuous effect + `ValueExpr::PowerOfTarget` snapshot +
  until-EOT cleanup expiry (Mightform's double-power; also generic +X/+Y-until-EOT). C16 adds
  `EventPattern::BecomesTargeted{filter, by_opponent}` + `GameEvent::Targeted`, fired at the 3
  target-lock sites → Surrak's "opponent targets your creature ⇒ draw" trigger (permanent half;
  creature-spell-on-stack half deferred). #43: Search/`SelectCards` candidates from the hidden
  library now reveal to the searcher in their view (fetch lands / Bushwhack / Erode render real
  names). 161 mtg-core tests green. Remaining caps: C14 warp (Mightform), Dyadrine (YouAttack +
  reflexive + quest counters), Keen-Eyed Curator (exile-types), + Surrak's inert can't-be-countered.
- **engine: C12 earthbend landed → two new reusable continuous/triggered subsystems.** The layer
  system now folds in **resolution-granted continuous effects** (`chars::ContinuousEffect` floating
  in `GameState`, the home for until-EOT pumps + animations) alongside printed statics, and the
  engine supports **delayed triggered abilities** (CR 603.7, `GameState.delayed_triggers` →
  `StackObjectKind::DelayedAbility` carrying concrete Actions). Earthbend (`Effect::Earthbend`)
  uses both: animate a target land to a 0/0 haste land-creature + N counters, with "when it dies or
  is exiled, return it tapped". **design flipped Ba Sing Se → `fully_implemented: true`** (b524244, no
  card change); the earthbend gap on Badgermole Cub + Earthbender Ascension is closed (those stay
  incomplete only on their other unbuilt mechanics — reflexive-mana trigger / quest-counter chain).
  **The Selesnya Landfall card-pool push is COMPLETE: all 18 distinct cards authored and the `selesnya`
  preset is the real mtggoldfish 60 (51 nonbasics + 7 Forest / 2 Plains, no padding).** Most are fully
  implemented; a few ship as faithful tracked-partials with advanced clauses deferred (Mightform's warp,
  Dyadrine's attack ability, Surrak's stack-spell half + can't-be-countered, Lumbering's crew,
  Earthbender's quest-chain, Badgermole's reflexive-mana) — none husked. Engine caps C1–C19 + C12/C15/C16/C17
  all landed; 168 mtg-core tests green. Remaining is **upgrade-only** (no new deck cards): C14 warp + the
  small deferred-clause caps above.
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
- **gym done with GYM_PLAN milestone 1 (#29): a learning agent that beats random.** On top of the
  M0 PyO3 boundary: a structured `gym.spaces.Dict` observation (globals + per-permanent/hand/stack
  rows with computed P/T, types/colors/keywords, status, counters, combat role, and **`grp_id`
  card-embedding ids**) and a **factored `Discrete` action space** with env-side autoregressive
  decomposition of targets/combat/multi-select/ordering + a legality mask (`obs.rs`/`codec.rs`/
  `layout.rs`, all swappable seams — Python reads shapes from the extension). `MtgEnv` is single-
  agent vs a fixed (random) opponent; a DeepSets policy (grp_id embedding + masked-mean-pool) trains
  via `MaskablePPO`. **Exit met:** win-rate vs random demo 0.615→0.77, burn-vs-bears 0.052→0.92;
  9 Rust + 9 pytest tests green. Gym-side only, zero engine changes. Next (needs greenlight): M2 —
  self-play league + snapshotting + vectorization (M3 resumable step API stays an `engine` item).
- **gym did GYM_PLAN milestone 0 (#22): the RL env came alive.** New `crates/mtg-py` (PyO3 +
  maturin `cdylib`, depends only on `mtg-core`, abi3 so it builds on the box's CPython 3.14) wraps
  the `Agent` boundary with a thread+channel `PyAgent` (port of `GreSessionAgent`, GYM_PLAN §2.2-A,
  zero engine changes): the game runs on its own OS thread, `decide` ships `(view, req)` over a
  channel and blocks, Python pulls/answers via `PyGame.step_to_decision`/`apply` (GIL released on
  the blocking recv). Observation encoder (`obs.rs`) and action codec (`codec.rs`, every request →
  flat `Discrete(64)` + legality mask) are behind clean swappable seams; the Python `MtgEnv` reads
  shapes from the extension so M1 can replace both without touching plumbing. **Exit criteria met:**
  11k random self-play games across 3 decks (auto-pass on+off), no panics, non-empty mask at every
  one of ~2.2M decisions, 100% card+zone conservation. Next (needs lead/user greenlight): M1 — real
  per-entity obs + factored action space + MaskablePPO; M3 resumable step API is an `engine`
  coordination item (snapshot/clone stubbed until then).
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
  **Lobby (landing page `/`)**: a server-side game registry configures *both* seats per game — each
  is a Human, a Random test agent, or RL (stubbed→random) — with create/join/list over a small REST
  API (`/api/games`); the game client moved to `/play` and binds to `?game=&seat=` (one tab per
  seat, so you can drive both sides). Rooms auto-start once every human seat connects; agent-vs-agent
  runs headless on create (spectating is a stub). The legacy `?p0=&p1=` quick game still works.
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
