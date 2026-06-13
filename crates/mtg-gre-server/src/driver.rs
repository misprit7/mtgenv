//! Match setup + wiring: build a lands-only [`GameState`] and run it through the engine's real
//! turn/priority loop ([`mtg_core::priority::Engine`], board task #7).
//!
//! This is deliberately thin — deck construction and seating the agents is the *client's* job;
//! all rules (turn structure, priority, SBAs, decking, masking of legal actions) live in
//! `mtg-core`. The CLI (M1) and the web server (M2) both call [`run_lands_game`], so the human
//! and the `RandomAgent` play through the exact same engine the RL backend will.
//!
//! (Earlier this file carried a stand-in loop while #7 was in flight; it now delegates to the
//! landed engine. Mulligans / choose-starting-player aren't issued yet because the engine
//! defers them — when it adds those decision points, they flow to these same agents for free.)

use mtg_core::agent::Agent;
use mtg_core::basics::{Phase, Zone};
use mtg_core::ids::PlayerId;
use mtg_core::priority::Engine;
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
    /// Per-step overrides of the Arena defaults (`true` = always stop here, `false` = never).
    pub overrides: Vec<(Phase, bool)>,
}

impl Default for Stops {
    fn default() -> Self {
        // Human play, MTGA defaults: auto-pass on, SmartStops on, resolve-own-stack on, default
        // persistent stops = your two main phases only (declare-attackers/blockers are forced
        // turn-based decisions, always presented anyway — not priority stops).
        Stops {
            auto_pass: true,
            full_control: false,
            smart_stops: true,
            resolve_own_stack: true,
            overrides: Vec::new(),
        }
    }
}

impl Stops {
    /// Paper Comprehensive-Rules: prompt at every priority window (auto-pass off).
    pub fn full_control() -> Self {
        Stops { auto_pass: false, ..Default::default() }
    }

    /// Whether this step is a stop (manual override, else the MP1/MP2 default).
    pub fn is_stop(&self, phase: Phase) -> bool {
        self.overrides
            .iter()
            .find(|(p, _)| *p == phase)
            .map(|(_, v)| *v)
            .unwrap_or(matches!(phase, Phase::PrecombatMain | Phase::PostcombatMain))
    }

    /// The MTGA-style decision: should the human be prompted at this priority window? Used
    /// client-side by `GreSessionAgent` so the policy honours live stop changes (no reset).
    pub fn should_ask(&self, phase: Phase, has_action: bool, own_on_top: bool) -> bool {
        if !self.auto_pass || self.full_control {
            return true; // paper-CR / full control: prompt everywhere
        }
        if self.is_stop(phase) {
            return true; // MP1/MP2 default or a manual stop
        }
        if own_on_top && self.resolve_own_stack {
            return false; // auto-pass to resolve your own spell/ability
        }
        self.smart_stops && has_action // SmartStops: prompt where you have a legal play
    }

    /// Effective per-priority-step stop state (the MTGA StopType vocabulary) for the UI phase bar.
    pub fn effective_steps(&self) -> Vec<(Phase, bool)> {
        use Phase::*;
        [
            Upkeep, Draw, PrecombatMain, BeginCombat, DeclareAttackers, DeclareBlockers,
            CombatDamage, EndCombat, PostcombatMain, End,
        ]
        .into_iter()
        .map(|p| (p, self.full_control || self.is_stop(p)))
        .collect()
    }
}

/// Apply a [`Stops`] config to the engine (for the given human seats) before running.
pub fn apply_stops(engine: &mut Engine, stops: &Stops, human_seats: &[PlayerId]) {
    engine.set_arena_auto_pass(stops.auto_pass);
    for &p in human_seats {
        engine.set_full_control(p, stops.full_control);
        engine.set_smart_stops(p, stops.smart_stops);
        engine.set_resolve_own_stack(p, stops.resolve_own_stack);
        for &(step, val) in &stops.overrides {
            engine.set_stop(p, step, Some(val));
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

/// Run one lands-only game between `agents` (indexed by seat) through `mtg-core`'s engine.
pub fn run_lands_game(agents: Vec<Box<dyn Agent>>, seed: u64) -> Outcome {
    run_state(lands_only_state(agents.len(), seed), agents)
}

/// Run one demo game (lands + creatures + burn) between `agents` through the engine.
pub fn run_demo_game(agents: Vec<Box<dyn Agent>>, seed: u64) -> Outcome {
    run_state(demo_state(seed), agents)
}

/// Build a game from optional per-seat preset deck names (`"burn"`/`"bears"`/`"demo"`); any
/// unset/unknown seat falls back to the demo deck. Used by the web server's deck picker.
pub fn state_for_decks(p0: Option<&str>, p1: Option<&str>, seed: u64) -> GameState {
    if p0.is_none() && p1.is_none() {
        return demo_state(seed);
    }
    let pick = |name: Option<&str>| {
        name.and_then(mtg_core::cards::preset_deck)
            .unwrap_or_else(mtg_core::cards::demo_deck)
    };
    let (d0, d1) = (pick(p0), pick(p1));
    mtg_core::cards::build_game(seed, &[&d0, &d1])
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
}
