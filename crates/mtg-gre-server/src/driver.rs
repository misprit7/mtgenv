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
use mtg_core::basics::Zone;
use mtg_core::ids::PlayerId;
use mtg_core::priority::Engine;
use mtg_core::state::{Characteristics, GameState};

/// How a game ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub winner: Option<PlayerId>,
    pub turns: u32,
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

/// Run one lands-only game between `agents` (indexed by seat) through `mtg-core`'s engine.
pub fn run_lands_game(agents: Vec<Box<dyn Agent>>, seed: u64) -> Outcome {
    let state = lands_only_state(agents.len(), seed);
    // The engine shuffles, deals opening hands, and runs the turn/priority loop to a result.
    let mut engine = Engine::new(state, agents);
    let winner = engine.run_game();
    Outcome {
        winner,
        turns: engine.state.turn_number,
    }
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
