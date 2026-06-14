# Work Log

Short, dated entries for future-agent consumption. Newest first. One line or a few bullets
per unit of meaningful progress. Keep it terse — detail lives in `docs/` and git history.

## 2026-06-13

- **engine:** **C11 cap: conditional enters-tapped rewrites** (b98afdc) — two `Rewrite` variants on
  `WouldEnterBattlefield(ItSelf)`: `EntersTappedUnless(Condition)` (check lands — taps iff condition
  fails, no choice) and `EntersTappedUnlessPay{life}` (shock lands — Confirm as it enters; pay→LoseLife
  +untapped, decline→tapped). Made `apply_rewrite` `&mut self` so the shock land can `self.ask`
  mid-ETB-replacement (the architectural bit). Tests cover all 4 branches. Unblocks Temple Garden +
  Ba Sing Se's tapped clause. 118 mtg-core green. (4 caps landed today: Sacrifice, ControllerOfTarget,
  attachments-view, C11.)

- **webui:** **Board visualizations + floating mana (user).** (1) **Stack→target arrows:** spells/
  abilities on the stack draw curved red SVG arrows (full-viewport overlay) from the stack card to
  each target (creature card / player panel), from the already-populated `StackObjView.targets`;
  cards carry `data-oid`/`data-sid`, players `data-pid`. Live in play/replay/spectate. (2)
  **Attachments behind host:** auras/equipment render stacked slightly offset BEHIND their host
  creature (not standalone), reading `ObjView.attachments` — engine populated it (fa808f9); verified
  on real data (a King Cheetah + attached obj 57 renders behind the host). (3) **Floating mana:** each
  player panel shows unspent `mana_pool` mana as Scryfall symbols under the life total. Commits
  a294feb (arrows+attach), 3ddd2ee (floating mana). **Pending engine (#36):** manual mana-ability
  activation — engine skips `is_mana` abilities at priority + auto-taps; spec'd offering
  `ActivateMana` + pay-from-pool-first + keep auto-tap default/no-new-stops (my options.rs already
  renders ActivateMana). **Perf:** also fixed `/api/replays` 3-7s→4ms (read only the `meta` prefix,
  not the full multi-MB files) so the gym's AI-training replays list instantly (commit cc1ae39).

- **engine:** **View: populate `ObjView.attachments`** (fa808f9, webui request) — `visible()` now fills
  each battlefield object's `attachments` with the ids of objects whose `attached_to` points at it
  (battlefield order, stable), instead of a hardcoded empty Vec; flows through `view_for` + `god_view`
  so the client renders auras/equipment behind their host on live games + replays. Test covers it. 116 green.
- **engine:** **C11–C18 cap: `PlayerRef::ControllerOfTarget(n)`** (632982e, atomic per design's bless) —
  resolves to the controller of the Nth object target, **snapshotted into `ResolutionCtx.target_controllers`
  at resolution start** (in `resolve_top`) so it survives that object leaving play mid-resolution.
  `eval_player` reads the snapshot; the static `conditions`/`pt_controller` matchers fall through their
  wildcards. Unblocks **Erode** ("Destroy target creature. Its controller may search…"). Test:
  `Sequence[Destroy, GainLife(ControllerOfTarget(0))]` gives life to the destroyed creature's controller,
  not the caster. 114 mtg-core tests green. (design's fetch lands landed first-try against the prior
  sac-cost cap, d96ec72.) Next: Dyadrine's `EventPattern::YouAttack`.
- **engine:** **C11–C18 cap: `CostComponent::Sacrifice` as an activation cost** (0451182) — pivoted to
  #21 and landed design's #1 gap. `can_pay_cost`/`pay_cost` (which only paid TapSelf/Loyalty) now
  resolve chooser-controlled battlefield permanents matching the spec filter (source-aware: `ItSelf`
  = the cost's source, so `{T}, Sacrifice this:` works) and sacrifice `spec.min` to the graveyard,
  asking `SelectCards` only on a genuine choice. Instant-speed activated abilities on lands already
  worked (`legal_priority_actions` enumerates them for any battlefield permanent, `Timing::Instant`),
  so this **fully unblocks the fetch lands** (Fabled Passage / Escape Tunnel) — design to author. Test
  covers Sacrifice-this. 111 mtg-core tests green. (Tracked-deferred per design: Fabled's "untap that
  land" = searched-permanent handle; Escape's "can't be blocked" = CantBeBlocked qualification.)
- **engine:** **Replay live frame sink** (9ec1fbf) — `Engine::set_replay_sink(Box<dyn FnMut(&ReplayFrame)>)`
  streams each god-view frame to the caller the instant it's captured (in `push_replay_frame`, on the
  game thread), unblocking webui's live god-view spectator (their `observe` only saw a masked
  `PlayerView`, never `GameState`). Installing a sink turns recording on. Non-`Send` `FnMut` (matches
  `Box<dyn Agent>`; engine is built+run on the game thread). Test streams a full game's frames live,
  count/labels matching the kept replay. Frame-size (~18MB/276-frame game) flagged to webui w/ options
  (gzip-on-serve / coarser-granularity knob / delta-frames). 110 mtg-core tests green.
- **gym:** **`mtg-py` replay accessor (REPLAY_PLAN §3 prep)** — exposes the engine's freshly-landed
  replay recording (a533720) through Python so M2 can export training self-play replays. `PyGame`
  gains `record_replay`/`replay_step` ctor args (game thread calls `set_replay_source(AiTraining
  {step})` + `record_replay(true)` before `run_game`, ships the `Replay` in `GameOver`) and
  `replay_json(created_at, names, decks) -> Optional[str]` (serde_json of `engine.replay()`, caller
  stamps the clock/names per the core's no-clock split; `None` unless recorded). Validated the
  locked schema end-to-end through Python: source `AiTraining{step}`, engine-filled `result`,
  god-view `frames` with labels ("game start" → "Turn N — P0 PrecombatMain" → "Mountain →
  Battlefield" …). 10 Rust + 12 pytest tests green. **The sampling/export LOOP stays in M2** (so
  recorded games are real self-play); this is just the isolated, low-risk accessor (lead-approved).
- **webui:** **Replay + omniscient-spectate feature (REPLAY_PLAN) — webui half mostly done.** Against
  engine's locked `crate::replay` schema: (1) **god-view viewer** — a new mode of the game client
  (`/play?replay=<id>`) that fetches `/api/replays/:id` and plays `frames[i].state` through the
  existing board render with NO masking (a `godToView` adapter), opponent hand face-up + both
  players' hand/ordered-library piles openable, playback bar (◀/⏯/▶, frames-per-sec slider, frame
  label; ←/→ + Space keys), no WebSocket. (2) **lobby** — finished games' button → god-view "▶ Replay";
  new "AI Training Replays" section listing `source:AiTraining` replays. (3) **serving** — `GET
  /api/replays` (flattened meta + id + frames, newest-first) + `GET /api/replays/:id` (full;
  sanitized id, traversal→400, missing→404) over the gitignored `data/replays/*.json` store. (4)
  **auto-save** — finished lobby games record an omniscient `Replay` (`driver::finish_game_with_replay`
  → `engine.record_replay`/`replay()`), stamped (names/decks/source=Human/created_at) and written to
  `data/replays/<game-id>.json`, so the finished-game button plays a real game back. Both clients in
  sync; tsc/vite + 13 web tests green. Playwright-verified end-to-end on a real 276-frame counters
  game + on mocked/synthetic frames. Commits c651ec7 (viewer), 6592a08 (lobby), 5bf5fec (serving),
  0dc9463 (auto-save).
- **webui:** **Live god-view spectating — replay/spectate feature COMPLETE (#31/#32/#33 done).** On
  engine's `set_replay_sink` (9ec1fbf): `spawn_game` installs a sink forwarding each omniscient
  `ReplayFrame` to the room `SpectateHub` (cached for late joiners; same frames feed the auto-save).
  New `ServerMsg::GodFrame{state:GodView,label}`; removed the old PlayerView-mirroring `SpectatorTee`.
  Spectator client runs `godMode` + renders god frames via the replay viewer's adapter (both hands
  face-up, ordered libraries openable, live "what happened" label). Playwright-verified: a spectator
  sees the opponent hand (7) + full ordered library (53, top-first) + live labels — zero hidden info.
  13 web tests green. Commit e9237a7. Frame-size mitigation (gzip-on-serve) deferred until it bites.
- **engine:** **Replay core landed (REPLAY_PLAN) — schema locked + recorder.** New `crate::replay`
  serde contract (posted to webui+gym first for parallel build): `GodView` (omniscient, every zone
  of every player face-up, libraries top-first) reusing `ObjView`/`CharacteristicsView`; `Replay`
  = `meta` + `frames`; `ReplayFrame{GodView,label}`; `ReplaySource` `Human|AiTraining{step}`;
  caller-stamped `created_at`/names/decks. `state::view::god_view(&GameState)` builds it (spectator-
  feed entry point). `Engine::record_replay(true)` captures a labelled frame per public event;
  `replay()`/`replay_frame_count()` expose it incrementally (live spectator) + final; `Outcome`/
  `EndReason` gained serde. 3 tests (wire-shape lock, omniscient-frame capture, JSON round-trip).
  Commit a533720; isolated from the lead's die-roll commit. (NB: a pre-existing failing test
  `opening_hands…skips_first_draw` is the lead's 497df25 die-roll, not the replay core — flagged.)
- **webui:** **3 card-UI fixes (user) + #30 badge closed on real data.** (1) **Stuck hover preview:**
  per-card mouseenter/leave listeners got orphaned when the board re-renders under the cursor (the
  hovered node is replaced, so mouseleave never fires → preview stuck). Replaced with a single global
  pointer tracker — cards carry their preview URL in `data-preview`; `refreshPreview()` re-derives the
  hovered card via `elementFromPoint` on every move AND after every render, so nothing can orphan it.
  (2) **Missing card art:** `card-art.json` was a stale 14-card list; regenerated for the full 38-card
  `starter_db` (all resolved on Scryfall, zero missing) — generator dict synced to the pool. (3)
  **Oversized card text:** `.c-rules` now line-clamps (4 lines board / 6 hand) so long oracle text
  (Mossborn Hydra) shows more but ellipsis-clips instead of blowing out of the frame. Embedded+Vite
  synced; tsc/vite clean. Playwright-verified live on a counters game: art 4/4, 0 frame-overflows
  (Sazh's Chocobo truncates), hover never stuck off-card across re-renders + re-acquires. Commit
  d9c7d43. **#30:** wire-level test (a99f23d) proves a real partial card (Lumbering Worldwagon)
  serializes to `fully_implemented:false` through the exact `ServerMsg`, a complete card to `true` —
  badge confirmed end-to-end on real data (render path already Playwright-verified). **#30 done.**
- **webui:** **Replay/spectate feature (REPLAY_PLAN.md) — taken, planned, coordinated.** Owning
  lobby+viewer+serving. Sent team-lead my plan + coordination asks (lock GodView/Replay serde field
  names — esp. ordered-library + both-hands exposure; the `god_view` spectator hand-off API; gitignore
  `data/replays/`). Scaffolding (lobby "AI Training Replays" section, `/play?replay=<id>` god-view
  playback viewer) waits on the locked schema; Rust serving/auto-save/god-view-spectator wait on schema
  + engine replay core. Tasks #31-33 filed.
- **webui:** **New `"counters"` preset deck** (user) + badge wire-test. A G/W landfall + +1/+1-counters
  midrange 60 built server-side (`driver::counters_deck`) from the implemented pool by `grp_id` —
  exercises mana dorks, a conditional dual, ETB draw, three landfall payoffs, counter-doubling +
  Hardened Scales, an anthem, equip/aura, a CDA search vehicle, and two tracked-incomplete cards (so
  the ⚠ badge shows on a real deck). `driver::resolve_deck` routes it through the lobby (now default),
  legacy `?p0/?p1`, and CLI. Commits ade8b5a, a99f23d.
- **engine:** **Subtypes/supertypes flipped from strings → generated enums (CR 205.3/4).** Lead
  directive; I drove the whole no-fallback flag-day flip in one green commit (5b9f63d), design
  parked. `Characteristics`/`ComputedChars.subtypes → Vec<Subtype>`, `supertypes → Vec<Supertype>`;
  `CardFilter::HasSubtype(Subtype)`/`Supertype(Supertype)`; `TokenSpec.subtypes: Vec<Subtype>`.
  Engine matching mostly survives unchanged (`Vec::contains` is generic); rewrote the string-literal
  checks (`mana::basic_land_type_color`, sba/priority Aura/Equipment) to enum matches. All card
  producers + builders migrated (`creature()` takes `CreatureType`; the 7 two-subtype bodies — Human
  Soldier, Elf Archer, … — set the full subtype line after via a `two_subtype_kw` helper / inline,
  preserving fidelity). Views Display-convert enums to the canonical type-line string so the wire/
  webui JSON is byte-identical (also fixed gre-server's `DeckCardView`). Snapshots regen'd. Full
  workspace green (106+13+9), no new warnings. Kills stringly-typed-subtype typo risk. **design to
  review cards/+effects/.**
- **engine:** **`fully_implemented` surfaced in the view (#30, engine side)** + **subtype enums landed
  (step 1).** (1) `CharacteristicsView` gained `fully_implemented: Option<bool>`, populated in
  `view.rs::chars_view` from `CardDef.fully_implemented` (design's 2fdaa77) via grp_id — `Some(true/
  false)` for real cards, `None` for engine objects; webui's ⚠ badge (⚠ iff `Some(false)`) now runs
  on live data. 3-case test. (665773d). (2) Subtypes/supertypes → enums: generated `crate::subtypes`
  (Subtype tagged + Supertype flat, Display/FromStr/serde-as-string) landed additively/green
  (924a321). The hard `Characteristics` field flip (Vec<String>→Vec<Subtype/Supertype>) + matching +
  CardFilter IR variants + every card def is one atomic commit (no additive bridge) — coordinating
  shapes + who-drives with design before the destructive sweep. View stays Vec<String>+Display so
  the wire is unchanged. 106 tests green.
- **webui:** **New `"counters"` preset deck** (user request — "a more complicated deck that uses
  more of the cards/functionality"). A G/W landfall + +1/+1-counters midrange 60 (24 land / 26
  creature / 10 noncreature), built server-side in `driver::counters_deck()` from the implemented
  pool by `grp_id` (kept in the server crate — composes engine cards by id rather than adding a
  `preset_deck` in card-agnostic `mtg-core`). Exercises a broad slice of the engine in one game:
  Llanowar Elves (mana dork) + Hushwood Verge (conditional dual), Elvish Visionary ETB-draw, three
  landfall payoffs (Sazh's Chocobo +1/+1, Mossborn Hydra counter-double, Icetill Explorer mill),
  Hardened Scales counter-replacement, Glorious Anthem static, Bonesplitter equip + Pacifism aura,
  Lumbering Worldwagon `*`/4 CDA + basic search, and keyword bodies — incl. two tracked-incomplete
  cards (Icetill, Worldwagon) so the ⚠ badge shows on a real deck. New `driver::resolve_deck` routes
  the name through the lobby picker (now defaults to `counters`), legacy `?p0/?p1`, and the CLI
  `preset`/`play`. Tests: deck is a legal 60, all ids resolve in `starter_db`, RandomAgent mirror
  plays to a winner (12 web tests green). Verified live: lobby lists it, agent-vs-agent counters
  game runs to `finished{winner:0}`. Commit ade8b5a.
- **webui:** **"not fully implemented" ⚠ card badge** (user/lead request, webui half). A yellow ⚠
  corner badge renders on any card whose view `chars.fully_implemented === false`, with a hover
  tooltip "Not fully implemented:\n<rules_text>" (the deferred clause the engine documents in
  rules_text). No JSON-projection change needed — board `chars` are `CharacteristicsView` serialized
  whole into `PlayerView`, so the field passes through automatically once engine adds it (rules_text
  already present). Wired **forward-compatibly**: strict no-op until the field exists (`undefined`/
  `true` show nothing — no false positives). Mirrored embedded + Vite (+ CSS re-sync). Verified via
  Playwright with synthetic `fully_implemented:false` injection: flagged cards show ⚠ + the
  deferred-clause tooltip; control (no field) shows zero badges. Auto-activates when
  `CardDef.fully_implemented`→view lands (task #30 = real-data pass then). Commit 10a31bf.
- **engine:** **C19 DONE (#28) — `mana_colors` shortcut fully retired; mana production is 100% IR.**
  With design's card-side migration complete (basics → intrinsic CR 305.6 subtype mana;
  Llanowar/Hushwood → `Activated{is_mana}`), removed the `mana_colors` field from `CardDef`, the
  fallback union in `mana::producible_colors`, and trimmed `is_mana_source` to the authored-ability
  check. Mana colour now comes ONLY from IR mana abilities + intrinsic basic-type mana (computed
  subtypes). Migrated the summoning-sick dork test to a real `{T}: Add {G}` ability; cleaned a
  pre-existing dead `CounterKind` import. 105 tests green, workspace builds. `ManaSpec.one_of`
  deferred by mutual agreement (no current/planned card needs constrained "add A or B" mana).
- **engine:** **CR 305.6 — basic-land-type mana is intrinsic, derived from computed subtypes**
  (C19/#28 follow-through, lead-flagged). `mana::mana_sources` now unions three colour sources:
  IR mana abilities (`Ability::Activated{is_mana}`, condition-aware), the **intrinsic** basic-type
  mana read from each permanent's COMPUTED subtypes (`Forest`→{G} … `Plains`→{W}, new
  `basic_land_type_color`), and the legacy `mana_colors` fallback. Reads post-layer subtypes, so
  animated lands / Spreading Seas / Urborg-style type changes carry mana for free; basics + typed
  duals now author as just their type line (no `mana_ability`, no `mana_colors`). New test
  `basic_land_type_mana_is_intrinsic_from_subtype`. 105 tests green, clippy clean. `mana_colors`
  removal + `ManaSpec.one_of` wiring still pending design dropping the shortcut from basics/lands.
- **gym:** **GYM_PLAN milestone 1 COMPLETE — obs encoder + factored action space + MaskablePPO
  beats random.** Replaced M0's flat obs/action with the real thing (gym-side only):
  `layout.rs` (shared entity ordering/sizes so obs row `i` ↔ action slot `i`), `obs.rs` (structured
  `Dict` obs — globals + per-permanent/hand/stack rows with computed P/T, types/colors/keywords,
  status, counters, combat role, **`grp_id` card-embedding ids**; `obs_spec()` so Python builds the
  space, never hard-coded), `codec.rs` (factored `Discrete(98)` vocab `COMMIT/HAND/PERM/PLAYER/
  STACK/MODE/COLOR/NUMBER/YES/NO` + an autoregressive `Interaction` state machine that decomposes
  targets/combat/multi-select/ordering into single-index sub-steps env-side, committing the batched
  `DecisionResponse` only at the end; rare value decisions use the engine's canonical default).
  `lib.rs` drives sub-steps (one engine request → N gym steps). Python: `MtgEnv` is single-agent vs
  a fixed (random) opponent so win-rate-vs-random is measurable; `policy.py` DeepSets extractor
  (grp_id embedding → per-row MLP → masked mean-pool); `train.py` MaskablePPO. **Exit met:** demo
  0.615→**0.770**, burn-vs-bears 0.052→**0.917** win-rate vs random. 9 Rust + 9 pytest tests green
  (incl. a ~20s learning-sanity test). Obs/codec stay swappable for M2/M4; needs zero engine changes.
- **webui:** **lobby spectating + per-decision delay** (user request). Read-only spectating:
  `/ws?game=<id>&spectate=1` subscribes to a per-room `SpectateHub` (a `tokio::broadcast` of seat-0
  view frames, fed by a `SpectatorTee` agent wrapping seat 0 that mirrors every `observe` to the
  hub) — late joiners get the cached current board immediately, then live frames, then GameOver. The
  existing game client renders the stream read-only (`👁 Spectating` banner, no prompts). Per-game
  **decision delay** (`delay_ms` in create form/REST): a `DelayAgent` wraps each non-human seat and
  `sleep`s before each decision, pacing the single-threaded engine so AI-vs-AI games are watchable;
  humans unaffected. Lobby Spectate button now live for non-finished games; ⏱ chip shows the delay;
  also added a DELETE endpoint + ✕ remove / Clear-finished (prior commit). Verified (WS + Playwright):
  delay=0 game finishes instantly → spectator gets cached final view + GameOver; delay=120 game
  streams paced live frames (13 over 2.5s, spread in time); late-join gets the board at t=0. 10 web
  tests green. Commits 197cfe0 (remove/clear), 7b204ab (spectate+delay).
- **design:** **Effect-IR for batch-1 caps + first real cards (Selesnya Landfall push).** Additive
  IR: `CardFilter::Supertype` + `Effect::Search.tapped` + `Effect::Fight` (06ce436);
  `StaticContribution::SetBasePTValue` (7a CDA) + `ManaCost.x` (27879eb); `ValueExpr::CountersOnSelf`
  (w/ engine, d95abe1). Authored 4 fidelity-clean cards (per-first-printing-set folders, each
  expect-tested): **Sazh's Chocobo** (fin) + **Mossborn Hydra** (fdn) (95cd0c8), **Icetill Explorer**
  (eoe) (8ce5ea1), **Lumbering Worldwagon** (dft `*`/4 CDA) (30d865a). **Fidelity standard (user):**
  no silent approximations — incomplete clauses TRACKED in-file, never shipped wrong. Tracked: Icetill
  land-play perms (C18), Lumbering **Crew 4**. **Held on engine caps:** Bushwhack (cast-time
  modal-targets), Erode (`PlayerRef::ControllerOfTarget`), Dyadrine (mana-spent value + "you attack"
  event). **NEXT:** the C19 mana migration engine handed off — `ManaSpec.one_of` + `basic_mana_ability`
  builder + migrate basics/Llanowar/lands off `mana_colors`, incl. Hushwood Verge's conditional `{W}`
  via the new `Condition::CountAtLeast`. **Purge:** holding the 6 card defs until engine clears the
  priority.rs test refs.
- **engine:** **C19 — mana production as first-class IR** (engine side, lead's priority). New
  `conditions.rs` — a pure `Condition` evaluator (`holds`: CountAtLeast/life/turn/All/AnyOf/Not).
  `mana.rs` now derives a source's producible colours from its `Ability::Activated{is_mana:true}`
  mana abilities (the `Effect::AddMana` ManaSpec), gated by `Restriction`/`Condition` — so a
  conditional mana ability (Hushwood Verge {W} iff you control a Forest/Plains) is only offered
  when legal; kept the `mana_colors` shortcut as a transitional fallback so existing lands don't
  regress. `Effect::AddMana` → mana pool (produces + any_color via ChooseColor). 104 mtg-core
  tests green, clippy clean. NEXT (design): add `ManaSpec.one_of` + `basic_mana_ability` builder +
  migrate basics/Llanowar/lands to the IR mana ability; then I wire one_of + remove the fallback.
- **engine:** **partial-card test purge DONE.** Per lead, cleared all test refs to the 6
  soon-deleted partial cards (Humility/Rancor/Fog Bank/Servant/Chandra/Healing Salve) from
  chars/mod.rs + priority.rs + combat/mod.rs. Coverage preserved via self-contained synthetic test
  CardDefs (a `synth_state()` helper: loyalty planeswalker, combat-damage-prevention 0/2,
  0/0-enters-with-counter — keeps the whiteboard replacement-pass tests incl. Hardened Scales +
  CR 616.1f — and a +2/+0-trample aura). Humility test dropped (Nature's Revolt covers 7b). 104
  mtg-core tests green. design is clear to delete the defs (pinged). Leftover: a "Rancor" doc
  comment in design's effects/target.rs (their file, harmless).
- **webui:** **lobby + per-seat agent assignment** (user request). New `lobby.rs`: a server-side
  game registry (`Arc<Lobby>` axum state) where each `Room` configures *both* sides — every seat is
  a `Human`, a `Random` test agent, or `Rl` (stubbed→random for now). REST `GET/POST /api/games`
  (+`/api/games/:id`); the lobby landing page (`/`, new self-contained `lobby_client.html`) lists
  games and creates them; the game client moved to `/play` and binds to `?game=<id>&seat=<n>` (one
  browser tab per seat — open two to play both sides). Rooms **auto-start when every human seat has
  connected** (agent-only games run on create). The rendezvous is one `Mutex<StartState>` (derived
  fullness, drain-to-spawn, double-claim reject, pre-start slot-vacate on disconnect). Load-bearing
  detail: `Box<dyn Agent>` isn't `Send`, so the room stores only `Send` channel *ingredients* per
  seat and the spawned engine thread builds the agents itself (mirrors the legacy path). Added
  `driver::room_engine` (per-seat stop handles) + `state_for_deck_names`; factored the socket loop
  into `server::run_player_socket` shared by legacy + lobby. Legacy `/ws?p0=&p1=` path preserved
  verbatim. Verified end-to-end (REST + WS + Playwright): agent-vs-agent finishes on create; a lone
  seat does NOT start + vacates on disconnect; 2-human auto-starts only after both connect and both
  drive it to GameOver; human+random plays like today; `/play?game=…&seat=0` shows "you are Player
  0"; legacy path still works. 10 web tests green. Commits fd9a72b, ffb820b.
- **gym:** **GYM_PLAN milestone 0 COMPLETE — PyO3 boundary + random self-play.** New crate
  `crates/mtg-py` (PyO3/maturin `cdylib`, depends only on `mtg-core`, abi3-py39 so it builds
  against the box's CPython 3.14): a `PyGame` handle + thread+channel `PyAgent` (port of
  `GreSessionAgent` — game runs on its own OS thread, each seat's `decide` ships `(view, req)` over
  a channel and blocks; Python pulls via `step_to_decision`, answers via `apply`; GIL released
  around the blocking recv). Swappable seams kept minimal but real: `obs.rs` (PlayerView→f32 global
  scalars, `OBS_DIM=54`) and `codec.rs` (every `DecisionRequest`→non-empty canonical legal-response
  list→flat `Discrete(ACTION_DIM=64)` + bool mask; decode clamps — illegal action impossible). Thin
  `python/mtgenv_gym/` (`MtgEnv(gym.Env)` reading dims from the extension, low-level self-play
  driver, smoke test, benchmark). **Exit criteria PASS**: 11k random games (lands/demo/burn-vs-bears,
  auto-pass on+off), 0 panics, **0 empty masks across ~2.2M decisions**, 100% card+zone conservation;
  ~10k–24k decisions/s/thread single-threaded. 6 Rust + 8 pytest tests green. M1 (real obs encoder +
  factored action space + PPO) swaps `obs.rs`/`codec.rs` with no plumbing change; snapshot/clone
  stubbed pending the M3 resumable step API (needs `engine` coordination).
- **engine:** **card-push batch 1 COMPLETE (C9b + C10)** — all of C1–C10 now landed. C9b dynamic
  P/T: `ValueExpr::CountersOnSelf` in eval_value (Mossborn Hydra "double the +1/+1 counters" =
  `PutCounters{SourceSelf, n: CountersOnSelf(+1/+1)}`); `StaticContribution::SetBasePTValue` as a
  layer-7a CDA in chars::compute (Lumbering Worldwagon's power = lands you control), with a chars-
  local ValueExpr evaluator. C10 X-costs: `ManaCost.x`; cast_spell asks `ChooseNumber(ChooseX)`
  bounded by affordable mana, pays generic + X·x, carries X on `StackObject.x`, and resolve_top
  sets `ctx.x` so `ValueExpr::X` reads it. (Also added the missing `CounterKind` import to
  effects/value.rs that design's `CountersOnSelf` addition needed.) 99 mtg-core tests green, clippy
  clean. **C1–C10 done**; design can author all Tier-1/2/3 cards + Mossborn/Lumbering/Dyadrine.
  Remaining: C11–C18 subsystems (dual lands, earthbend, crew, warp, target-trigger, exile-types,
  land permissions) — shaped per-card with design. NOTE: temporarily added a placeholder
  crates/mtg-py/src/lib.rs to unblock a workspace-wide cargo break (gym's crate had Cargo.toml but
  no lib.rs); gym has since filled it in.
- **engine:** **card-push capabilities, batch 2 (C5, C7, C8)**. C7 Modal: added an interactive
  resolution interpreter — `resolve_effect` now drives `interpret()` (asks `ChooseModes`, resolves
  the chosen modes) while still materializing pure leaves with the shared target cursor. C5 Search:
  `interpret_search` (SelectCards → move picks to `ZoneDest` → shuffle a searched library); honors
  `Effect::Search.tapped` (fetch lands enter tapped) + `CardFilter::Supertype` (basic-land filter) —
  both added by design. C8 Fight: `Effect::Fight` → two simultaneous Noncombat `Damage` actions
  (each creature's power to the other; deathtouch/lethal interact via the one whiteboard). 96
  mtg-core tests green, clippy clean. CAVEAT noted to design: Modal chooses its mode at RESOLUTION;
  a modal mode that TARGETS (Bushwhack's fight mode) needs cast-time mode+target selection (the
  Fight effect itself works via locked targets) — a follow-up. Batch-1/2 capabilities C1–C9a + C6–C8
  are ready for card authoring. Pending: C9b (dynamic P/T CDAs), C10 (X costs) — IR asks sent;
  C11–C18 subsystems.
- **webui:** **new default stop set** (user request): your Main 1 + Main 2 (engine Arena default,
  own-turn) PLUS the opponent's Begin-Combat + End step (the instant-speed windows). Made
  `driver::Stops.overrides` per-`(step, own_turn)` (`Vec<(Phase, bool, bool)>`), seeded the two
  opp-side stops in `Stops::default`, applied via `set_stop_side`/`set_override`; server `?stops=`
  param now layers on the defaults and supports a `Name@you|opp:val` side suffix (bare = both
  sides). CLI `stop` cmd toggles both sides + shows the side. Verified the wire echo matches exactly
  (MP1/MP2 mine-only, BeginCombat/End opp-only, rest off). 10 web tests green. Commit 3c4d5a2.
- **engine:** **card-push capabilities, batch 1 (C1–C4, C6, C9-Count)** — all additive-only, no IR
  change (the Effect/ValueExpr nodes existed but were no-ops). C1: mana.rs gates a creature mana
  dork by summoning sickness (CR 302.6). C2: `Effect::PutCounters` → `Action::AddCounters`. C3:
  `Effect::Mill` → real (top N library → graveyard). C4: **landfall** via a new watching-enters
  trigger scan in `collect_triggers` — on any permanent's ETB it scans battlefield permanents for
  `PermanentEnters(filter)` triggers, filter evaluated relative to the watcher's controller (so "a
  land you control enters" works; no `LandEntersControlled` variant needed — proposed reuse to
  design). C6: `Effect::CreateToken` → real (token onto battlefield, summoning-sick; TokenSpec
  keywords still a vanilla no-op). C9: `ValueExpr::Count` → real (count objects in a zone by
  filter + optional controller, e.g. lands you control). 91 mtg-core tests green, clippy clean.
  Pending in this batch: C5 (Search), C7 (Modal), C8 (Fight) need resolution-time agent decisions.
- **lead:** **Card-pool push kicked off — Standard Selesnya Landfall** (60-card deck, 18 unique
  nonbasics). Built a **SQLite card index** (`scripts/build_card_index.py` → `data/scryfall/
  cards.sqlite`, one row per printing, indexed by name/oracle_id) and wired it into `setup.sh`, so
  card lookups are instant instead of `jq`-ing the 550MB JSON (~2 min/pass). Spec + per-card data +
  ease tiers + the interpreter-capability list (C1–C18) → `docs/plans/SELESNYA_LANDFALL_CARDS.md`.
  Delegated on disjoint file seams: **engine** = interpreter capabilities (whiteboard/effects),
  **design** = `cards/` refactor (misc/ + per-set folders by first-printing set) + card authoring.
- **lead:** Implemented the **London mulligan (CR 103.5)** in `start_game::run_mulligans` — rounds
  in turn order, shuffle-hand-into-library + redraw on mulligan, bottom one card per mulligan on
  keep, all through the `Agent` boundary (`Mulligan` + `SelectCards{BottomForMulligan}`). RandomAgent
  keeps every hand (a coin-flip mull is noise for self-play; keeping consumes no RNG so seeded games
  stay deterministic). The web/CLI projection already handled both requests. 85 mtg-core tests green
  + a new scripted-mulligan test.
- **webui:** **play-UX polish from rapid user feedback.** (1) Stop dots are now full-width
  clickable rows (much bigger hit targets, 12px dots) instead of tiny circles; (2) the two
  per-step dots reordered to opponent-on-top / **you-on-bottom** (matches board layout); (3)
  **target-selection feedback** — fixed the missing `button.opt.sel` style so a chosen option is
  visibly highlighted, made the board's player panels click-targets (glow + "🎯 target" badge) for
  player-targeting choices, selected board cards get a 🎯 corner badge, and the prompt shows a
  "🎯 Chosen: …" summary; (4) **Space submits a valid in-progress selection** (declare
  attackers/targets/order/number), not just pass; (5) **SmartStops OFF by default** (`Stops::default`
  + server flag defaults tied to it) — users found "stop wherever I *could* cast" too chatty.
  Verified via Playwright (20 dots, opp/you legend, 18px rows, Bolt→player target highlights on
  panel+button+summary, Space→`picks` frame) + WS (smart_stops=false echo). NOTE: fully honoring
  "no priority on non-stop steps regardless of castable spells" needs an engine tweak — with
  SmartStops off the engine still falls back to `is_unimportant_step`, so it prompts at the
  opponent's main phases + combat steps when you hold an instant. Proposal sent to engine (make
  SmartStops-off auto-pass all non-stop empty-stack windows). 10 web tests green. Commit 81bb8e7.
- **webui:** **per-turn-side stops in the UI + MTGA keybindings.** Consumed the engine's new
  per-`(Phase, own_turn)` stop API: `ServerMsg::Stops.per_step` now carries both sides as
  `(step, on_my_turn, on_opp_turn)`; `ClientMsg::SetStop` gained an `own` flag;
  `stops_msg` zips `effective_steps(true/false)`, `SetStop`→`set_override(step, own, …)`,
  `engine_with_stops` seeds both sides. Phase bar renders **two stop dots per step** (top = your
  turn, bottom = opponent's, dashed ring = opp, independently clickable) with a you/opp legend —
  the user can stop on their own draw but not the opponent's. **Keybindings** (per
  `../mtga-re/docs/priority_stops.md`): **Space** = pass priority / take the sole forced option;
  **Enter** = pass through all of THIS turn's remaining priority stops (mirrors the GRE's
  `autoPassPriority=Yes`/`AutoPassOption.Turn` — a per-turn hold shown by a badge, lapses next turn,
  still surfaces real choices like targets/blocks); **Esc** cancels the hold. Mirrored across the
  embedded + Vite clients (CSS re-synced, dist rebuilt). Verified end-to-end: WS shows 3-tuple
  `per_step` + independent per-side toggles + Arena default (MP1 = your-turn only); a my-Upkeep stop
  set over the socket actually yields an upkeep prompt (engine honors it); Playwright confirms 20
  dots + legend, per-side dot toggle, Space→one `{pass:true}` frame, Enter→multi-stop pass-through +
  badge. 10 web tests green, workspace builds. Commit 8699b79.
- **engine:** **per-turn-side stops** (webui request). `StopConfig.overrides` now keyed by
  `(Phase, own_turn)` (`own_turn = seat == active_player`) so a seat can stop on its OWN draw step
  but not the opponent's. New `StopConfig::stop_for(step, own_turn)` primitive; `set_override` +
  `effective_steps` gain an `own_turn` arg; `Engine::set_stop` stays both-sides (back-compat, CLI
  unchanged), new `Engine::set_stop_side` for one side. Arena default unchanged (MP1/MP2 stop only
  on your own turn). 84 mtg-core tests green, clippy clean. mtg-gre-server callers (3 sites) are
  webui's to adapt — flagged the signatures.
- **webui:** **migrated the web stop policy onto the engine's `stops_handle`** (removes the
  duplicated client-side policy from the earlier entry). The game thread builds the engine via new
  `driver::engine_with_stops(state, agents, human, &Stops)` (auto-pass ON for the human seat) and
  hands the seat's `Arc<Mutex<StopConfig>>` back to the socket task over a tokio oneshot — the
  Engine never leaves the thread (`dyn Agent` isn't `Send`); only the Send handle crosses.
  `GreSessionAgent::decide` is now a plain round-trip (engine elides trivial windows itself);
  `SetStop`→`StopConfig::set_override`, `SetOption`→pub fields, echo reads `StopConfig`. Deleted
  `driver::Stops::{should_ask,is_stop,effective_steps}` (the web/CLI now share the ONE engine
  policy — no drift). Verified: engine auto-passes Upkeep/Draw (first prompt at Main 1), live
  Upkeep toggle lights + yields a real Upkeep prompt with no reset (WS + Playwright); decklist
  intact; 10 web tests green. Commit d21fc14.
- **engine:** task #14 checkpoint 4 — **planeswalkers** (DONE → #14 complete). Groundwork:
  `Characteristics.loyalty` (printed), enters-with-loyalty on battlefield entry (CR 306.5b),
  CR 704.5i 0-loyalty SBA, `Object.used_once_per_turn` (CR 606.3) reset each turn. **4a Loyalty
  abilities:** loyalty cost via design's `CostComponent::Loyalty(i32)` in `can_pay_cost`/`pay_cost`
  (+N adds counters, −N removes, −N gated on loyalty≥N); once-per-turn-per-planeswalker enforced;
  loyalty abilities flow through the existing activated-ability path (sorcery-speed, controller-only
  by construction). Card: Chandra, Pyrogenius (+2 deals 2 to each opponent, −3 deals 4 to target
  creature; −10 ultimate deferred). **4b Attackable:** `declare_attackers` offers the defending
  player's planeswalkers as attack targets (CR 508.1a); `apply_damage` to a planeswalker removes that
  many loyalty counters (CR 120.3/306.8), 0-loyalty SBA handles death. starter_db 37→38, 84 mtg-core
  tests green, clippy clean. IR: only `CostComponent::Loyalty` (design). **#14 done** across 4
  checkpoints (combat keywords → rest of keywords → auras/equipment → planeswalkers). Deferred across
  #14: ward (cost IR), shroud, Rancor recursion, general enchant restrictions, planeswalker ultimates.
- **webui:** **live mid-game stop toggling** (fixed: was resetting the game) + **debug library
  peek**. Stops: moved the MTGA auto-pass/stops POLICY client-side into `GreSessionAgent` over a
  shared `Arc<Mutex<driver::Stops>>` the socket mutates on `SetStop`/`SetOption`; engine auto-pass
  stays OFF so the agent elides windows per the live config — clicking a phase-bar step or top-bar
  toggle now changes stops at the next priority window with **no reset** (verified browser +
  Playwright). (Engine also landed a server-side `stops_handle`; webui doesn't consume it.) Library:
  a player can't see their own library (hidden info; would leak draws to the RL agent if put in
  `PlayerView` — design flagged), so the peek is a **static starting decklist snapshotted server-side
  from `GameState` before run** (`ServerMsg::Decklist`, grouped by card, order discarded) → grouped
  MTGO-style deck-view modal on the Lib pile. Also: removed artist credits, hover→full-card image,
  MTGO phase bar (all 12 steps) above the hand, clickable GY/exile zone viewers. Mirrored across the
  no-build embedded client + the Vite client (rebuilt dist; CSS synced). 10 mtg-gre-server tests green.
  NOTE: `embedded_client.html` is baked via `include_str!` in server.rs — editing it only re-bakes
  when server.rs's mtime changes (touch it before rebuild); and when `web/dist/` exists the server
  serves the Vite build, not the embedded client (keep both in sync).
- **engine:** task #14 checkpoint 3 — **auras + equipment** (the attachment subsystem), in three
  commits. **3a Auras:** `Object.attached_to` + detach-on-zone-change in `move_object`;
  `CardFilter::AttachedHost` matcher (source-relative, like ItSelf → resolves to the host) so a
  "enchanted creature gets …" static lives in the normal global gather scan; Aura spells target a
  creature at cast and enter the battlefield attached on resolution (illegal target → graveyard);
  CR 704.5 Aura fall-off SBA. Card: Rancor (+2/+0 & trample). **3b Equipment + the activated-ability
  path:** `legal_priority_actions` now enumerates non-mana activated abilities (masked by timing /
  restriction / cost / legal target); `activate_ability` puts it on the stack, chooses targets, pays
  the cost (`pay_cost`); `resolve_top` runs Activated alongside Triggered; `Effect::Attach` →
  `Action::AttachTo`; `target_candidates` now honours `ControlledBy` (equip's "creature you control");
  CR 704.5q equipment-unattaches SBA. Card: Bonesplitter (Equip {1}, +2/+0). **3c Qualification
  dimension:** `ComputedChars.qualifications` gathered through layer 6 (`StaticContribution::
  Qualification`), read by combat — `CantAttack` (declare_attackers) + `CantBlock` (can_block). Card:
  Pacifism. starter_db 34→37. 78 mtg-core tests green, clippy clean. IR (design): used
  `Effect::Attach{what,to}`, `CardFilter::AttachedHost`. Deferred: Rancor's return-to-hand recursion
  (needs ReturnToHand + dies-trigger for non-creatures); general enchant restrictions (Auras hardcode
  "enchant creature"). Next: (4) planeswalkers.
- **engine:** webui ask — **live mid-game stop toggling**. `Engine::stops_handle(p) ->
  Arc<Mutex<StopConfig>>`: a UI session holds the handle and toggles a seat's stops from another
  thread; the engine re-reads the shared config at every priority window (no game reset). Moved
  `auto_pass` into per-seat `StopConfig` so the handle is self-contained (1:1 with webui's
  `driver::Stops`); added `StopConfig::set_override`/`effective_steps`. `stop_config(p)` now returns
  an owned clone. Lets webui delete its duplicated stop policy and let the engine own it.
- **engine:** task #14 checkpoint 2 — **rest of the evergreen keywords**. Haste (combat
  `declare_attackers` ignores summoning sickness when the creature has Haste); flash
  (`legal_priority_actions` treats Flash as instant-speed timing); hexproof (`targetable_by` +
  `target_candidates` exclude an opponent-controlled Hexproof permanent, but its own controller
  may still target it); indestructible-vs-destroy confirmed (added `Effect::Destroy` IR →
  `Action::Destroy`, whose `apply_action` skips Indestructible). New IR: only `Effect::Destroy`
  (additive; coordinated mentally with the existing vocabulary — no breaking change). Scryfall-
  verified single-keyword cards added: Raging Goblin (Haste), King Cheetah (Flash), Gladecover
  Scout (Hexproof), Darksteel Myr (colorless Artifact Creature, Indestructible), Murder
  ({1}{B}{B} "Destroy target creature"). starter_db 29→34. 71 mtg-core tests green, clippy
  clean. **Deferred:** ward (needs a cost-payment IR — will ping `design` at checkpoint 3/4)
  and shroud (niche, not in the Keyword enum). Next: (3) auras+equipment, (4) planeswalkers.
- **engine:** task #14 checkpoint 1 — **evergreen COMBAT keywords**. Added PRINTED keywords
  (`Characteristics.keywords: Vec<Keyword>`, seeded into `chars::compute` so the layer system
  layers grants/removes on top). Implemented: first strike & double strike (combat damage now
  has the two-substep model, CR 510.4, with an SBA pass between steps so a creature killed in
  the first-strike step doesn't deal back); trample (assign lethal to blockers, excess to the
  defender); deathtouch (any nonzero damage lethal — `Object.dealt_deathtouch` + SBA 704.5h,
  and 1 counts as lethal for trample assignment); lifelink (source's controller gains the
  damage); vigilance (doesn't tap to attack); menace (single block dropped → stays unblocked);
  defender (can't attack); + indestructible now prevents the lethal-damage/deathtouch SBA (not
  the toughness-0 one). Flying/reach were done in M5. Scryfall-verified single-keyword cards:
  Elvish Archers, Fencing Ace, Argothian Swine, Typhoid Rats, Child of Night, Alaborn
  Grenadier, Alley Strangler, Wall of Stone. 67 mtg-core tests green, workspace green, clippy
  clean. No effects/ change (Keyword enum already complete). Next checkpoints: (2) rest of
  evergreen keywords (haste/flash/hexproof/shroud/ward/indestructible-confirm), (3)
  auras+equipment, (4) planeswalkers.
- **gym:** refreshed `docs/plans/GYM_PLAN.md` to a current spec (was a stale sketch). Removed all
  Forge references (abandoned prior attempt); re-anchored on PyO3+maturin + the *implemented* `Agent`
  boundary (`agent.rs`: 21 `DecisionRequest` variants, `PlayerView`, `RandomAgent`). Key updates:
  obs space maps the real `PlayerView`/`CharacteristicsView` (`grp_id` card-embedding ids,
  hidden-info masking inherited from `view_for`); factored action vocab + boolean mask
  (MaskablePPO), reusing `options.rs`'s 5-shape projection; reward sparse-terminal + annealed
  potential shaping; auto-pass/stops (already in `priority.rs`) as the episode-length lever.
  **Resumable-step API**: documented two shapes — (A) thread+channel `PyAgent` reusing the proven
  `GreSessionAgent` bridge (zero engine change, ship now), (B) a true re-entrant `resume`/`submit`
  engine API (coordinate with `engine`, spec only). Testing reframed to CR expect-tests + captured
  MTGA logs (not a cross-engine oracle). Milestones 0→4. **Spec only — awaiting user review before
  any implementation.**
- **webui:** big UI/UX pass on `mtg-gre-server` (many small commits). (1) **MTGO-style web board**:
  real MTG card frames (name/mana/art/type/rules/PT), opponent-top/you-bottom layout, hand at the
  bottom, left rail (life + Library/Graveyard/Exile piles → click to view zones), readable game-log
  transcript, click-to-act legal-card highlighting; both the no-build embedded client and the TS/Vite
  build kept in sync. (2) **Real card data from Scryfall**: batch-resolved art manifest
  (`resolve-card-art.py` → `card-art.json`, served at `/card-art.json`, zero runtime API calls —
  loads cached CDN images), official mana/tap **symbol SVGs** in costs + inline in oracle text,
  artist + WotC attribution. Exact mana pips + oracle text from design's `mana_cost`/`rules_text`.
  (3) **Deck picker** (demo/burn/bears) in CLI (`play burn bears`, `preset`) + web (top-bar links /
  `?p0=&p1=`). (4) **#12 stops (MTGA auto-pass)**: auto-pass on by default for human play (prompts
  ~5× fewer: 144→29 CLI / 181→70 web), CLI commands (`autopass`/`smartstops`/`fullcontrol`/
  `resolvestack`/`stop <step>`/`stops`) + web toggle links, and a live per-step stops panel reading
  engine's `PlayerView.stops` ("stops: MP1, MP2 · smart"). (5) **#13 layers**: computed P/T +
  granted keywords render for free (Bears 2/2 →+Anthem 3/3 →+Levitation 3/3 [Flying]; Humility →1/1);
  CLI board render now shows P/T+keywords; aliases for anthem/levitation/humility. Server runs on
  :8080. All via the public Agent boundary (the formal client = `GreSessionAgent`/`HumanAgent`; the
  browser is transport+presentation below the line). Full workspace green throughout.
- **engine:** task #13 (ENGINE_PLAN milestone 5) — the **CR-613 layer system** (continuous
  effects), prototype-first (4 snapshot commits). New `chars/`: `ComputedChars` +
  `compute(state, id)` = base ⊕ layered static effects, the 7-layer framework with timestamps
  (613.7: `Object.timestamp` assigned on battlefield entry; effects sorted within a sublayer).
  Layers populated/validated: **6** (Grant/RemoveKeyword), **7b** (SetBasePT), **7c** (ModifyPT
  + ±1/±1 counters); 4/5 (type/color) framework-present, 1–3 (copy/control/text) deferred;
  613.8 dependency = timestamp ordering (genuine card-pair case deferred). Cards (Scryfall):
  **Glorious Anthem** (7c +1/+1, stacks), **Levitation** (6 grant flying), **Humility** (7b set
  base 1/1, modeling only the P/T clause). Dirty→recompute discipline: `GameState.chars_cache`
  + `chars_dirty`, marked on zone/counter changes, rebuilt by the agenda's recompute step
  (`recompute_continuous`); `computed(id)` reads the cache when fresh, else computes on demand
  (always correct). Integrated into **SBA** (death uses computed toughness), **combat**
  (computed power/lethal + **flying evasion** — a granted-flying creature is unblockable by
  non-flyers), and the **view** (battlefield P/T/keywords shown computed, so the UI sees
  anthems/counters). 58 mtg-core tests green (anthem stacking, grant-flying→combat, set-base
  then anthem then counter sublayer order, dirty discipline); workspace green, clippy clean.
  No effects/ change (built over design's `StaticContribution` IR). Deferred: layers 1–5 copy/
  control/text, CDAs, a genuine 613.8 dependency case, RemoveAllAbilities (Humility's other half).
  - **M5-gen: affects-reads-COMPUTED (CR 613.8)** — `compute` no longer pre-filters statics by
    base characteristics; each layer re-checks applicability against the type set computed
    through PRIOR layers. So a land turned into a creature (layer 4) is seen as a creature by
    a layer-6/7 effect. Validated: **Nature's Revolt** ("all lands are 2/2 creatures") + Glorious
    Anthem → your land-creature is 3/3 (anthem reads its computed Creature type), opponent's
    land-creature stays 2/2; combat creature-eligibility now uses computed type too (a
    land-creature can attack/block). 59 mtg-core tests green. STILL deferred (need new
    subsystems/IR, scoped for the lead): layer 1 copy (copiable values + ETB copy), layer 2
    control (auras + computed-controller refactor + `SetController`), layer 3 text, 7a CDAs,
    7d switch, intra-layer 613.8 dependency ordering.
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
  - **PlayerView.stops echo** (with design): design added `PlayerView.stops: Option<StopStateView
    { full_control, smart_stops, resolve_own_stack, per_step: Vec<(Phase,bool)> }>` (settings-echo,
    render-only, self-only); engine populates it via `Engine::view_for_seat` (per_step from
    `is_stop` over priority-granting steps), so webui renders the real per-seat stop config
    instead of hardcoded defaults. None when the profile is off. The SET half (live toggling)
    stays deferred — it's the real settings sub-protocol, not game state.
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
