//! Match setup + wiring: build a lands-only [`GameState`] and run it through the engine's real
//! turn/priority loop ([`mtg_core::priority::Engine`], board task #7).
//!
//! This is deliberately thin — deck construction and seating the agents is the *client's* job;
//! all rules (turn structure, priority, SBAs, decking, masking of legal actions) live in
//! `mtg-core`. The CLI (M1) and the web server (M2) both call [`run_lands_game`], so the human
//! and the `RandomAgent` play through the exact same engine the RL backend will.
//!
//! (Earlier this file carried a stand-in loop while #7 was in flight; it now delegates to the
//! landed engine. The engine now issues London mulligans at game start — `Mulligan` per seat, then
//! `SelectCards{BottomForMulligan}` on keep — which flow to these same agents for free (options.rs
//! already projects both; the human gets Keep/Mulligan, then a bottom-N selection). Choose-starting-
//! player isn't issued yet; when the engine adds it, it flows the same way.)

use std::sync::{Arc, Mutex};

use mtg_core::agent::Agent;
use mtg_core::basics::{Phase, Zone};
use mtg_core::ids::PlayerId;
use mtg_core::priority::{Engine, StopConfig};
use mtg_core::state::{Characteristics, GameState};

/// How a game ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub winner: Option<PlayerId>,
    pub turns: u32,
}

/// MTGA-style stop configuration the client applies to the engine before a game runs. With
/// `auto_pass` on, the human's `decide()` is only called at stops + meaningful decisions (the
/// engine elides trivial priority windows) — much less tedious than paper-CR's every-window prompt.
#[derive(Debug, Clone)]
pub struct Stops {
    /// Arena auto-pass profile on (default for human play) vs paper-CR every-window prompting.
    pub auto_pass: bool,
    /// Stop at every priority window (overrides the default stops).
    pub full_control: bool,
    /// SmartStops (MTGA default ON): stop at any step where you have a legal play.
    pub smart_stops: bool,
    /// ResolveMyStackEffects (MTGA default ON): auto-pass while your own object is on top of the
    /// stack (don't re-prompt to respond to yourself).
    pub resolve_own_stack: bool,
    /// Per-`(step, own_turn)` overrides of the Arena defaults (`own_turn` = the seat's own turn;
    /// `true` = always stop here, `false` = never). Applied at session start over the engine's
    /// `arena_default_stop`.
    pub overrides: Vec<(Phase, bool, bool)>,
}

impl Default for Stops {
    fn default() -> Self {
        // Human play: auto-pass on, resolve-own-stack on. SmartStops is OFF by default here
        // (diverges from MTGA's on-by-default): users found "stop at every step where I *could* cast
        // something" (e.g. holding a Shock with red mana open) far too chatty — they want priority
        // only at steps they've actually marked. Flip it back on with the "smart" toggle.
        //
        // Default stop set = your Main 1 + Main 2 (from the engine's Arena default, own-turn only)
        // PLUS the opponent's Beginning of Combat + End step (seeded here) — the classic instant-
        // speed windows you want to act in on their turn. Everything else is off both sides.
        // (declare-attackers/blockers are forced turn-based decisions, presented anyway.)
        Stops {
            auto_pass: true,
            full_control: false,
            smart_stops: false,
            resolve_own_stack: true,
            overrides: vec![
                (Phase::BeginCombat, false, true), // opponent's beginning of combat
                (Phase::End, false, true),         // opponent's end step
            ],
        }
    }
}

impl Stops {
    /// Paper Comprehensive-Rules: prompt at every priority window (auto-pass off).
    pub fn full_control() -> Self {
        Stops { auto_pass: false, ..Default::default() }
    }
}

// NOTE: the auto-pass/stops POLICY (which windows actually prompt) and the phase-bar's effective
// stop state both live in the engine's `StopConfig` (CR-correct masking is the engine's job). This
// `Stops` is just the parsed/transport carrier — the CLI applies it via [`apply_stops`] and the web
// applies it onto a live [`mtg_core::priority::Engine::stops_handle`] (see [`engine_with_stops`]).

/// Apply a [`Stops`] config to the engine (for the given human seats) before running.
pub fn apply_stops(engine: &mut Engine, stops: &Stops, human_seats: &[PlayerId]) {
    engine.set_arena_auto_pass(stops.auto_pass);
    for &p in human_seats {
        engine.set_full_control(p, stops.full_control);
        engine.set_smart_stops(p, stops.smart_stops);
        engine.set_resolve_own_stack(p, stops.resolve_own_stack);
        // #36: a human seat may manually tap mana sources (the engine offers an ActivateMana per
        // untapped source). Non-intrusive — casting still auto-taps the rest; floated mana pays first.
        engine.set_manual_mana(p, true);
        for &(step, own, on) in &stops.overrides {
            engine.set_stop_side(p, step, own, Some(on));
        }
    }
}

/// Like [`run_state`] but applies a stop config first (MTGA-style auto-pass for human play).
pub fn run_state_with(
    state: GameState,
    agents: Vec<Box<dyn Agent>>,
    stops: &Stops,
    human_seats: &[PlayerId],
) -> Outcome {
    let mut engine = Engine::new(state, agents);
    apply_stops(&mut engine, stops, human_seats);
    let winner = engine.run_game();
    Outcome {
        winner,
        turns: engine.state.turn_number,
    }
}

/// The five basic land names, dealt round-robin into each library.
const BASICS: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];
/// Library size per seat (small so a lands-only game ends by deck-out quickly). The engine
/// draws the opening hand from this, so it must exceed the opening hand size.
const LIBRARY_SIZE: usize = 14;

/// Build a fresh lands-only [`GameState`]: `num_players` seats, each with a round-robin basic-land
/// library (the engine deals opening hands itself). Shared by [`run_lands_game`] and the CLI's
/// quick `play` command.
pub fn lands_only_state(num_players: usize, seed: u64) -> GameState {
    let mut state = GameState::new(num_players, seed);
    for seat in 0..num_players as u32 {
        let pid = PlayerId(seat);
        for i in 0..LIBRARY_SIZE {
            let name = BASICS[i % BASICS.len()];
            state.add_card(pid, Characteristics::basic_land(name), Zone::Library);
        }
    }
    state
}

/// A two-player demo game with the engine's starter card DB: a Gruul deck of lands, vanilla
/// creatures, and burn — so casting, the stack, and combat are all exercised.
pub fn demo_state(seed: u64) -> GameState {
    mtg_core::cards::two_player_demo_game(seed)
}

/// Run a prepared `state` through `mtg-core`'s engine with `agents` (indexed by seat). The
/// engine shuffles, deals opening hands, and runs the turn/priority/combat loop to a result.
pub fn run_state(state: GameState, agents: Vec<Box<dyn Agent>>) -> Outcome {
    let mut engine = Engine::new(state, agents);
    let winner = engine.run_game();
    Outcome {
        winner,
        turns: engine.state.turn_number,
    }
}

/// Build the engine for a human **web** session and hand back the `human` seat's live stop handle
/// (with `stops` applied and auto-pass per the config). The engine owns the auto-pass/stops policy;
/// the socket task holds the returned handle and toggles overrides mid-game (`set_override`) — the
/// engine re-reads the shared config at the next priority window, so stops change with no reset.
/// Returns the (not-yet-run) engine; call [`finish_game`] on the game thread to play it out.
pub fn engine_with_stops(
    state: GameState,
    agents: Vec<Box<dyn Agent>>,
    human: PlayerId,
    stops: &Stops,
) -> (Engine, Arc<Mutex<StopConfig>>) {
    let engine = Engine::new(state, agents);
    let handle = engine.stops_handle(human);
    {
        let mut c = handle.lock().unwrap();
        c.auto_pass = stops.auto_pass;
        c.full_control = stops.full_control;
        // TECH-DEBT (backlog, spec'd to engine): the web stop policy currently lives CLIENT-side as
        // a filter (`priorityAutoPass`) over the engine's surfaced SUPERSET — we force smart_stops
        // on (marked-phases + any window with a play + opp-respond) and the client narrows it to the
        // real rule (stop iff [marked phase OR opp spell on top] AND you have a usable non-mana
        // action). This DIVERGES from the "stops policy lives in the engine" law; engine will
        // canonicalize `should_auto_pass` to the exact rule + drop these flags, then this force and
        // the client filter both go away. Until then, force smart on regardless of the carrier.
        c.smart_stops = true;
        c.resolve_own_stack = stops.resolve_own_stack;
        c.manual_mana = true; // #36: offer this human seat ActivateMana per untapped source
        // Seed the per-`(step, own_turn)` stop overrides (default set + any URL overrides). The user
        // then toggles individual sides live (`SetStop`), which mutate this same shared config.
        for &(step, own, on) in &stops.overrides {
            c.set_override(step, own, Some(on));
        }
    }
    (engine, handle)
}

/// Like [`engine_with_stops`] but for a **multi-seat lobby game**: builds the engine, applies each
/// human seat's [`Stops`] to its own `StopConfig`, and returns the (not-yet-run) engine plus each
/// human seat's live stop handle (so each seat's socket task can toggle its own stops mid-game). The
/// engine itself never leaves the game thread (`dyn Agent` isn't `Send`); only the handles cross.
pub fn room_engine(
    state: GameState,
    agents: Vec<Box<dyn Agent>>,
    humans: &[(PlayerId, Stops)],
) -> (Engine, Vec<(PlayerId, Arc<Mutex<StopConfig>>)>) {
    let engine = Engine::new(state, agents);
    let mut handles = Vec::with_capacity(humans.len());
    for (seat, stops) in humans {
        let handle = engine.stops_handle(*seat);
        {
            let mut c = handle.lock().unwrap();
            c.auto_pass = stops.auto_pass;
            c.full_control = stops.full_control;
            // Force smart on (engine surfaces a superset of windows; the web client filters down to
            // the actual stop rule). See `engine_with_stops`.
            c.smart_stops = true;
            c.resolve_own_stack = stops.resolve_own_stack;
            c.manual_mana = true; // #36: offer this human seat ActivateMana per untapped source
            for &(step, own, on) in &stops.overrides {
                c.set_override(step, own, Some(on));
            }
        }
        handles.push((*seat, handle));
    }
    (engine, handles)
}

/// Play a prepared engine to completion (used by the web path, which runs it on its own thread
/// after extracting the live stop handle via [`engine_with_stops`]).
pub fn finish_game(mut engine: Engine) -> Outcome {
    let winner = engine.run_game();
    Outcome {
        winner,
        turns: engine.state.turn_number,
    }
}

/// Like [`finish_game`] but records an omniscient [`Replay`](mtg_core::replay::Replay) of the whole
/// game (god-view frame per public event). Returns the outcome plus the recorded replay — the
/// engine fills seats + result; the caller stamps `source`/`created_at`/player names+decks and
/// persists it (so the lobby's finished-game "Replay" button can play it back).
pub fn finish_game_with_replay(mut engine: Engine) -> (Outcome, mtg_core::replay::Replay) {
    engine.record_replay(true);
    let winner = engine.run_game();
    let replay = engine.replay();
    let outcome = Outcome {
        winner,
        turns: engine.state.turn_number,
    };
    (outcome, replay)
}

/// Run one lands-only game between `agents` (indexed by seat) through `mtg-core`'s engine.
pub fn run_lands_game(agents: Vec<Box<dyn Agent>>, seed: u64) -> Outcome {
    run_state(lands_only_state(agents.len(), seed), agents)
}

/// Run one demo game (lands + creatures + burn) between `agents` through the engine.
pub fn run_demo_game(agents: Vec<Box<dyn Agent>>, seed: u64) -> Outcome {
    run_state(demo_state(seed), agents)
}

/// The deck names this server offers, in picker order. The first three are the engine's trivial
/// starter piles (`mtg_core::cards::preset_deck`); `"counters"` is the richer server-local deck
/// built by [`counters_deck`]. Shared source of truth for the lobby/CLI pickers.
pub const DECK_NAMES: &[&str] = &["selesnya", "counters", "demo", "burn", "bears"];

/// Every `(grp_id, exact card name)` that can appear in a game this server serves: the union of all
/// selectable decks ([`DECK_NAMES`]) resolved against the engine's [`starter_db`]. Sorted, unique.
///
/// This is the canonical "cards that need art" set — a card a player can ever see is exactly a card
/// in some deck, since `build_game` only ever draws a player's cards from their deck. It's the one
/// source of truth shared by the startup art-coverage check ([`crate::server::missing_card_art`])
/// and the `dump-cards` resolver helper (which feeds `resolve-card-art.py`), so adding a card to a
/// deck automatically pulls it into both the warning and the art fetch.
///
/// [`starter_db`]: mtg_core::cards::starter_db
pub fn deck_card_pool() -> Vec<(u32, String)> {
    let db = mtg_core::cards::starter_db();
    let mut pool: std::collections::BTreeMap<u32, String> = std::collections::BTreeMap::new();
    for name in DECK_NAMES {
        let Some(deck) = resolve_deck(name) else { continue };
        for grp in deck {
            if let Some(def) = db.get(grp) {
                pool.entry(grp).or_insert_with(|| def.chars.name.clone());
            }
        }
    }
    pool.into_iter().collect()
}

/// A *much* richer preset than the trivial burn/bears/demo piles: a **Selesnya (G/W) landfall +
/// +1/+1-counters midrange** deck assembled from the implemented card pool. Where the three
/// starter decks are one or two cards stamped 40–60 times, this one is built to exercise a broad
/// slice of the engine in a single hand-played game:
///
/// - **Mana:** Llanowar Elves (a mana-dork activated ability + summoning sickness) and Hushwood
///   Verge (a conditional dual land — its `{W}` only unlocks once you control a Forest/Plains).
/// - **ETB / dies triggers:** Elvish Visionary ("draw a card" on enter).
/// - **Landfall (three payoffs):** Sazh's Chocobo (+1/+1 counter per land), Mossborn Hydra
///   (*doubles* its counters), Icetill Explorer (mill — a *tracked-incomplete* card → ⚠ badge).
/// - **Counter synergy / replacement:** Hardened Scales (one extra +1/+1 counter each time) stacks
///   with the hydra and chocobo; the hydra also enters-with-a-counter via a self-replacement.
/// - **Layer system:** Glorious Anthem (a +1/+1 anthem static).
/// - **Equipment + activated equip / Auras:** Bonesplitter (equip ability) and Pacifism
///   (can't-attack/can't-block aura — soft removal).
/// - **CDA + library search:** Lumbering Worldwagon (`*`/4 power-equals-lands vehicle that
///   searches a basic onto the battlefield — also tracked-incomplete Crew → ⚠ badge).
/// - **Keyword bodies:** trample / double strike / flash / vigilance / indestructible creatures.
///
/// Built from the public `grp_id` constants (this lives in the server crate, so it composes the
/// engine's cards by id rather than adding a `preset_deck` entry in card-agnostic `mtg-core`).
pub fn counters_deck() -> Vec<u32> {
    use mtg_core::cards::dft::lumbering_worldwagon::LUMBERING_WORLDWAGON;
    use mtg_core::cards::dsk::hushwood_verge::HUSHWOOD_VERGE;
    use mtg_core::cards::eoe::icetill_explorer::ICETILL_EXPLORER;
    use mtg_core::cards::fdn::mossborn_hydra::MOSSBORN_HYDRA;
    use mtg_core::cards::fin::sazhs_chocobo::SAZHS_CHOCOBO;
    use mtg_core::cards::grp::*;
    use mtg_core::cards::lea::llanowar_elves::LLANOWAR_ELVES;

    // (grp_id, copies) — 60 cards: 24 land / 26 creature / 10 noncreature.
    let spec: &[(u32, usize)] = &[
        // Lands (24): green-heavy with a white splash; Hushwood Verge is the conditional dual.
        (FOREST, 10),
        (PLAINS, 8),
        (HUSHWOOD_VERGE, 6),
        // Creatures (26).
        (LLANOWAR_ELVES, 4),  // {G} mana dork (activated mana ability)
        (SAZHS_CHOCOBO, 3),   // landfall: +1/+1 counter
        (ELVISH_VISIONARY, 3), // ETB: draw a card
        (GRIZZLY_BEARS, 2),   // vanilla beater
        (MOSSBORN_HYDRA, 3),  // landfall: double counters; trample; enters with a counter
        (ARGOTHIAN_SWINE, 2), // trample
        (ICETILL_EXPLORER, 2), // landfall: mill (tracked-incomplete → ⚠)
        (FENCING_ACE, 2),     // double strike
        (KING_CHEETAH, 2),    // flash
        (ALABORN_GRENADIER, 1), // vigilance
        (DARKSTEEL_MYR, 1),   // indestructible artifact creature
        (LUMBERING_WORLDWAGON, 1), // */4 CDA vehicle + basic-land search (Crew incomplete → ⚠)
        // Noncreature (10).
        (HARDENED_SCALES, 3), // replacement: +1 extra +1/+1 counter
        (GLORIOUS_ANTHEM, 2), // static anthem (+1/+1)
        (PACIFISM, 3),        // aura: can't attack or block
        (BONESPLITTER, 2),    // equipment (+2/+0, equip {1})
    ];
    let mut deck = Vec::new();
    for &(id, n) in spec {
        deck.extend(std::iter::repeat(id).take(n));
    }
    deck
}

/// Resolve a web/CLI deck name to a `grp_id` list: the server-local complex decks first
/// (`"counters"`), then the engine's [`mtg_core::cards::preset_deck`] (`burn`/`bears`/`demo`).
/// `None` for an unknown name (callers fall back to the demo deck).
pub fn resolve_deck(name: &str) -> Option<Vec<u32>> {
    match name.to_ascii_lowercase().as_str() {
        "counters" => Some(counters_deck()),
        // "selesnya"/"landfall" fall through to the engine's official implemented-landfall
        // preset (mtg_core::cards::selesnya_landfall_deck) — NOT the server-local counters deck.
        other => mtg_core::cards::preset_deck(other),
    }
}

/// Build a game from optional per-seat deck names (`"counters"`/`"burn"`/`"bears"`/`"demo"`); any
/// unset/unknown seat falls back to the demo deck. Used by the web server's deck picker.
pub fn state_for_decks(p0: Option<&str>, p1: Option<&str>, seed: u64) -> GameState {
    if p0.is_none() && p1.is_none() {
        return demo_state(seed);
    }
    let pick = |name: Option<&str>| {
        name.and_then(resolve_deck)
            .unwrap_or_else(mtg_core::cards::demo_deck)
    };
    let (d0, d1) = (pick(p0), pick(p1));
    mtg_core::cards::build_game(seed, &[&d0, &d1])
}

/// Build a game from N per-seat deck names (`"counters"`/`"burn"`/`"bears"`/`"demo"`); any unknown
/// name falls back to the demo deck. Used by the lobby (arbitrary seat count). Decks are `grp_id`
/// lists.
pub fn state_for_deck_names(seed: u64, names: &[&str]) -> GameState {
    let decks: Vec<Vec<u32>> = names
        .iter()
        .map(|n| resolve_deck(n).unwrap_or_else(mtg_core::cards::demo_deck))
        .collect();
    let refs: Vec<&[u32]> = decks.iter().map(|d| d.as_slice()).collect();
    mtg_core::cards::build_game(seed, &refs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::agent::RandomAgent;

    #[test]
    fn random_vs_random_terminates_with_a_winner() {
        // The boundary guarantees only-legal options, so two RandomAgents always finish a
        // lands-only game (by deck-out), deterministically per seed.
        let agents: Vec<Box<dyn Agent>> =
            vec![Box::new(RandomAgent::new(1)), Box::new(RandomAgent::new(2))];
        let outcome = run_lands_game(agents, 42);
        assert!(outcome.winner.is_some(), "game should produce a winner");
    }

    #[test]
    fn outcome_is_deterministic_for_seed() {
        let make = || -> Vec<Box<dyn Agent>> {
            vec![Box::new(RandomAgent::new(7)), Box::new(RandomAgent::new(9))]
        };
        let a = run_lands_game(make(), 123);
        let b = run_lands_game(make(), 123);
        assert_eq!(a, b);
    }

    #[test]
    fn counters_deck_is_60_and_every_card_is_known() {
        // The richer server-local deck must be a legal 60 and every grp_id must resolve in the
        // engine's starter DB (so `build_game` actually puts all of them in the library).
        let deck = counters_deck();
        assert_eq!(deck.len(), 60, "counters deck should be 60 cards");
        let db = mtg_core::cards::starter_db();
        for &g in &deck {
            assert!(db.get(g).is_some(), "grp_id {g} not in starter_db");
        }
        // Resolves by name (server-local), and is distinct from the trivial demo deck.
        assert_eq!(resolve_deck("counters").unwrap().len(), 60);
        assert_eq!(resolve_deck("COUNTERS").unwrap().len(), 60);
        assert_ne!(resolve_deck("counters"), resolve_deck("demo"));
        // Falls through to the engine's presets for the simple names, None for nonsense.
        assert_eq!(resolve_deck("burn").unwrap().len(), 60);
        assert!(resolve_deck("nonesuch").is_none());
    }

    #[test]
    fn counters_mirror_terminates_with_a_winner() {
        // Two RandomAgents on the complex deck still drive to a result (the boundary only ever
        // offers legal options), so the deck is engine-playable end-to-end.
        let agents: Vec<Box<dyn Agent>> =
            vec![Box::new(RandomAgent::new(1)), Box::new(RandomAgent::new(2))];
        let state = state_for_deck_names(42, &["counters", "counters"]);
        let outcome = run_state(state, agents);
        assert!(outcome.winner.is_some(), "counters mirror should produce a winner");
    }

    /// Walk a serialized `PlayerView` JSON tree and collect `name → fully_implemented` for every
    /// `chars` object (one that carries both `name` and the `fully_implemented` key).
    fn collect_flags(
        v: &serde_json::Value,
        out: &mut std::collections::HashMap<String, Option<bool>>,
    ) {
        match v {
            serde_json::Value::Object(m) => {
                if let (Some(serde_json::Value::String(name)), Some(flag)) =
                    (m.get("name"), m.get("fully_implemented"))
                {
                    out.insert(name.clone(), flag.as_bool());
                }
                for child in m.values() {
                    collect_flags(child, out);
                }
            }
            serde_json::Value::Array(a) => a.iter().for_each(|c| collect_flags(c, out)),
            _ => {}
        }
    }

    #[test]
    fn fully_implemented_flag_reaches_the_wire_for_a_real_partial_card() {
        // Real-data verification of the ⚠ "not fully implemented" badge (task #30): a board with a
        // genuinely tracked-incomplete card (Surrak, Elusive Hunter — can't-be-countered clause
        // deferred; the lone remaining partial in the pool) and a complete vanilla (Grizzly Bears).
        // Project the seat view, wrap it in the exact `ServerMsg::Event` the server pushes,
        // serialize, and assert the per-card flag the web client reads.
        use mtg_core::agent::GameEvent;
        use mtg_core::basics::{Phase, Zone};
        use mtg_core::cards::grp;
        use mtg_core::cards::tdm::surrak_elusive_hunter::SURRAK_ELUSIVE_HUNTER;
        use mtg_core::state::view::view_for;
        use std::sync::Arc;

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(mtg_core::cards::starter_db()));
        let (surrak, bear) = {
            let db = state.card_db();
            (
                db.get(SURRAK_ELUSIVE_HUNTER).unwrap().chars.clone(),
                db.get(grp::GRIZZLY_BEARS).unwrap().chars.clone(),
            )
        };
        state.add_card(PlayerId(0), surrak, Zone::Battlefield);
        state.add_card(PlayerId(0), bear, Zone::Battlefield);

        let view = view_for(&state, PlayerId(0));
        let msg = crate::protocol::ServerMsg::Event {
            event: GameEvent::PhaseBegan { turn: 1, phase: Phase::PrecombatMain, active: PlayerId(0) },
            view,
        };
        let json = serde_json::to_value(&msg).unwrap();
        let mut flags = std::collections::HashMap::new();
        collect_flags(&json, &mut flags);

        // The partial card serializes as `false` (client renders ⚠ + deferred-clause tooltip); the
        // complete card as `true` (no badge). This is the exact JSON the client parses.
        assert_eq!(
            flags.get("Surrak, Elusive Hunter"),
            Some(&Some(false)),
            "tracked-incomplete card must reach the wire as fully_implemented:false"
        );
        assert_eq!(
            flags.get("Grizzly Bears"),
            Some(&Some(true)),
            "fully-implemented card must reach the wire as fully_implemented:true"
        );
    }

    #[test]
    fn badgermole_bonus_makes_warp_mightform_castable() {
        // The user's exact hand-play scenario, behind the engine's Badgermole point-fix (#56,
        // fdfea6c). Board: control Badgermole Cub + Llanowar Elves + a Forest, all untapped, with
        // Mightform Harmonizer in hand. Tapping Llanowar (a creature) yields {G}{G} via Badgermole's
        // "+{G} per creature tapped", so Llanowar + Forest = 3 mana → Warp Mightform {2}{G} is
        // affordable; the hard cast {2}{G}{G} (4 mana) stays unaffordable until a 4th source. This
        // asserts the EXACT option strings the web client renders — `legal_actions` projected
        // through `options::prompt_for`, i.e. the `decide` frame the cast menu reads.
        use mtg_core::agent::{DecisionRequest, RandomAgent};
        use mtg_core::basics::{Phase, Zone};
        use mtg_core::cards::eoe::mightform_harmonizer::MIGHTFORM_HARMONIZER;
        use mtg_core::cards::grp::FOREST;
        use mtg_core::cards::lea::llanowar_elves::LLANOWAR_ELVES;
        use mtg_core::cards::tla::badgermole_cub::BADGERMOLE_CUB;
        use mtg_core::state::view::view_for;
        use std::sync::Arc;

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(mtg_core::cards::starter_db()));
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain; // sorcery speed, empty stack → creature spells legal
        let (bm, ll, fo, mf) = {
            let db = state.card_db();
            (
                db.get(BADGERMOLE_CUB).unwrap().chars.clone(),
                db.get(LLANOWAR_ELVES).unwrap().chars.clone(),
                db.get(FOREST).unwrap().chars.clone(),
                db.get(MIGHTFORM_HARMONIZER).unwrap().chars.clone(),
            )
        };
        state.add_card(PlayerId(0), bm, Zone::Battlefield); // Badgermole: +{G} per creature tapped
        state.add_card(PlayerId(0), ll, Zone::Battlefield); // Llanowar: {T}: add {G} (a creature)
        state.add_card(PlayerId(0), fo, Zone::Battlefield); // Forest: {T}: add {G} (a land)
        state.add_card(PlayerId(0), mf, Zone::Hand); // Mightform: {2}{G}{G}, Warp {2}{G}

        let view = view_for(&state, PlayerId(0));
        let engine = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(1)), Box::new(RandomAgent::new(2))],
        );
        let actions = engine.legal_actions(PlayerId(0));
        let prompt = crate::options::prompt_for(
            &view,
            &DecisionRequest::Priority { actions, can_pass: true },
        );

        // Warp cast now offered (was blocked before the fix): Badgermole makes 3 mana reachable.
        assert!(
            prompt.options.iter().any(|o| o.contains("Warp Mightform Harmonizer")),
            "Warp Mightform {{2}}{{G}} must be castable via the Badgermole bonus. Options: {:?}",
            prompt.options
        );
        // The {2}{G}{G} hard cast needs a 4th mana (e.g. an earthbent Forest) — not offered at 3.
        assert!(
            !prompt.options.iter().any(|o| o.starts_with("Cast Mightform Harmonizer")),
            "hard cast needs 4 mana; only 3 available, so it must NOT be offered. Options: {:?}",
            prompt.options
        );
    }

    #[test]
    fn badgermole_plus_earthbent_forest_makes_mightform_hard_castable() {
        // Case (b) of the Badgermole check: once the Forest is EARTHBENT into a land-creature, both
        // Llanowar AND the Forest tap as creatures → {G}{G} each via Badgermole = 4 mana → the full
        // {2}{G}{G} hard cast of Mightform is now affordable (in addition to Warp). We model the
        // earthbent Forest by giving the Forest the Creature type (what earthbend does).
        use mtg_core::agent::{DecisionRequest, RandomAgent};
        use mtg_core::basics::{CardType, Phase, Zone};
        use mtg_core::cards::eoe::mightform_harmonizer::MIGHTFORM_HARMONIZER;
        use mtg_core::cards::grp::FOREST;
        use mtg_core::cards::lea::llanowar_elves::LLANOWAR_ELVES;
        use mtg_core::cards::tla::badgermole_cub::BADGERMOLE_CUB;
        use mtg_core::state::view::view_for;
        use std::sync::Arc;

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(mtg_core::cards::starter_db()));
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let (bm, ll, mut fo, mf) = {
            let db = state.card_db();
            (
                db.get(BADGERMOLE_CUB).unwrap().chars.clone(),
                db.get(LLANOWAR_ELVES).unwrap().chars.clone(),
                db.get(FOREST).unwrap().chars.clone(),
                db.get(MIGHTFORM_HARMONIZER).unwrap().chars.clone(),
            )
        };
        fo.card_types.push(CardType::Creature); // earthbent: the Forest is now a land-creature
        fo.power = Some(2);
        fo.toughness = Some(2);
        state.add_card(PlayerId(0), bm, Zone::Battlefield);
        state.add_card(PlayerId(0), ll, Zone::Battlefield);
        state.add_card(PlayerId(0), fo, Zone::Battlefield);
        state.add_card(PlayerId(0), mf, Zone::Hand);

        let view = view_for(&state, PlayerId(0));
        let engine = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(1)), Box::new(RandomAgent::new(2))],
        );
        let prompt = crate::options::prompt_for(
            &view,
            &DecisionRequest::Priority { actions: engine.legal_actions(PlayerId(0)), can_pass: true },
        );
        // 4 creature-tap mana now available → the {2}{G}{G} hard cast IS offered (plus Warp).
        assert!(
            prompt.options.iter().any(|o| o.starts_with("Cast Mightform Harmonizer")),
            "with 4 mana (Llanowar + earthbent Forest, each {{G}}{{G}}) the hard cast must be \
             offered. Options: {:?}",
            prompt.options
        );
        assert!(
            prompt.options.iter().any(|o| o.contains("Warp Mightform Harmonizer")),
            "Warp is still offered too. Options: {:?}",
            prompt.options
        );
    }

    #[test]
    fn ba_sing_se_earthbend_taps_itself_plus_three_other_lands() {
        // #57/#59 fix: Ba Sing Se's "{2}{G}, {T}: Earthbend 2" must tap ITSELF for the {T} PLUS
        // 3 OTHER lands for the {2}{G} — its own {T} cost can't also pay a {G} of the {2}{G} (the
        // bug double-counted it, tapping only itself + 2). We drive the activation through the real
        // engine (scripted agent) and read the tapped lands off the resulting PlayerView — exactly
        // what the web board renders (rotated/.tapped cards).
        use mtg_core::agent::{
            Agent, DecisionRequest, DecisionResponse, ObjView, PlayableAction, PlayerView,
        };
        use mtg_core::basics::{Phase, Zone};
        use mtg_core::cards::grp::FOREST;
        use mtg_core::cards::tla::ba_sing_se::BA_SING_SE;
        use mtg_core::ids::ObjId;
        use std::cell::RefCell;
        use std::rc::Rc;
        use std::sync::Arc;

        struct Scripted {
            ba: ObjId,
            cap: Rc<RefCell<Option<PlayerView>>>,
            activated: bool,
        }
        impl Agent for Scripted {
            fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::Priority { actions, can_pass } => {
                        if !self.activated {
                            if let Some(i) = actions.iter().position(|a| {
                                matches!(a, PlayableAction::Activate { source, .. } if *source == self.ba)
                            }) {
                                self.activated = true;
                                return DecisionResponse::Action(i as u32);
                            }
                        } else if self.cap.borrow().is_none() {
                            *self.cap.borrow_mut() = Some(view.clone()); // post-activation: taps done
                        }
                        if *can_pass {
                            DecisionResponse::Pass
                        } else {
                            DecisionResponse::Index(0)
                        }
                    }
                    DecisionRequest::ChooseTargets { .. } => DecisionResponse::Pairs(vec![(0, 0)]),
                    DecisionRequest::DeclareAttackers { .. } => DecisionResponse::Indices(vec![]),
                    DecisionRequest::DeclareBlockers { .. } => DecisionResponse::Pairs(vec![]),
                    DecisionRequest::SelectCards { min, .. } => {
                        DecisionResponse::Indices((0..*min as u32).collect())
                    }
                    DecisionRequest::ChooseNumber { min, .. } => DecisionResponse::Number(*min),
                    DecisionRequest::Mulligan { .. } => DecisionResponse::Bool(false),
                    DecisionRequest::ChooseStartingPlayer { .. } => DecisionResponse::Index(0),
                    _ => DecisionResponse::Index(0),
                }
            }
        }

        let mut state = GameState::new(2, 7);
        state.set_card_db(Arc::new(mtg_core::cards::starter_db()));
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let (ba, fo) = {
            let db = state.card_db();
            (
                db.get(BA_SING_SE).unwrap().chars.clone(),
                db.get(FOREST).unwrap().chars.clone(),
            )
        };
        let ba_id = state.add_card(PlayerId(0), ba, Zone::Battlefield);
        // 4 other lands → 3 tap for {2}{G}, 1 stays untapped (proves "itself + 3", not "itself + 2").
        for _ in 0..4 {
            state.add_card(PlayerId(0), fo.clone(), Zone::Battlefield);
        }
        // small libraries so the game terminates quickly after the capture.
        for _ in 0..4 {
            state.add_card(PlayerId(0), fo.clone(), Zone::Library);
        }
        for _ in 0..2 {
            state.add_card(PlayerId(1), fo.clone(), Zone::Library);
        }

        let cap = Rc::new(RefCell::new(None));
        let mut engine = Engine::new(
            state,
            vec![
                Box::new(Scripted { ba: ba_id, cap: cap.clone(), activated: false }),
                Box::new(mtg_core::agent::RandomAgent::new(2)),
            ],
        );
        engine.skip_opening_deal(); // play the hand-built board as-is (no shuffle/deal)
        engine.run_game();

        let captured = cap.borrow();
        let view = captured.as_ref().expect("Ba Sing Se's earthbend should have been activated");
        let (mut tapped_lands, mut total_lands) = (0, 0);
        for o in &view.battlefield {
            if let ObjView::Visible { chars, controller, status, .. } = o {
                if *controller == PlayerId(0) && chars.card_types.iter().any(|t| t == "Land") {
                    total_lands += 1;
                    if status.tapped {
                        tapped_lands += 1;
                    }
                }
            }
        }
        assert_eq!(total_lands, 5, "board is Ba Sing Se + 4 Forests");
        assert_eq!(
            tapped_lands, 4,
            "Ba Sing Se's {{T}} + 3 OTHER lands for {{2}}{{G}} = 4 tapped (the #57/#59 fix); the \
             old double-count bug would tap only 3 (itself + 2)"
        );
    }

    #[test]
    fn manual_mana_seat_is_offered_activatemana_marked_is_mana() {
        // #36: a seat with manual mana ON is offered an ActivateMana per untapped source, which the
        // projection labels "Tap … for mana" + flags `is_mana` so the client (a) lights up the land
        // to click and (b) never treats a mana tap as a reason to stop. Mix a land-in-hand (PlayLand,
        // NOT mana) with an untapped Forest on the battlefield (ActivateMana, mana) to check the flag.
        use mtg_core::agent::{DecisionRequest, RandomAgent};
        use mtg_core::basics::{Phase, Zone};
        use mtg_core::cards::grp::FOREST;
        use mtg_core::state::view::view_for;
        use std::sync::Arc;

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(mtg_core::cards::starter_db()));
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let fo = state.card_db().get(FOREST).unwrap().chars.clone();
        state.add_card(PlayerId(0), fo.clone(), Zone::Battlefield); // untapped → ActivateMana
        state.add_card(PlayerId(0), fo, Zone::Hand); // a land to play → PlayLand (not mana)

        let view = view_for(&state, PlayerId(0));
        let mut engine = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(1)), Box::new(RandomAgent::new(2))],
        );
        engine.set_manual_mana(PlayerId(0), true); // the #36 switch the web session flips for humans
        let actions = engine.legal_actions(PlayerId(0));
        let prompt = crate::options::prompt_for(
            &view,
            &DecisionRequest::Priority { actions, can_pass: true },
        );

        // The mana tap is offered, labelled, and flagged; the land-play is offered but NOT flagged.
        let tap = prompt.options.iter().position(|o| o == "Tap Forest for mana");
        let play = prompt.options.iter().position(|o| o.starts_with("Play land"));
        assert!(tap.is_some(), "manual mana must offer an ActivateMana. Options: {:?}", prompt.options);
        assert!(play.is_some(), "the land in hand is still playable. Options: {:?}", prompt.options);
        assert_eq!(prompt.is_mana.len(), prompt.options.len(), "is_mana parallels options");
        assert!(prompt.is_mana[tap.unwrap()], "the mana tap is flagged is_mana");
        assert!(!prompt.is_mana[play.unwrap()], "the land-play is NOT flagged is_mana");
    }
}
