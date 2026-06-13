//! Thin CLI entrypoint for the headless engine: a lands-only self-play harness
//! (ENGINE_PLAN milestone 2). Two `RandomAgent`s pass priority through full turns until
//! a player decks out (CR 704.5b). Usage: `mtg-cli [seed] [library_size]`.

use std::env;

use mtg_core::agent::{Agent, RandomAgent};
use mtg_core::basics::Zone;
use mtg_core::ids::PlayerId;
use mtg_core::priority::Engine;
use mtg_core::state::{Characteristics, GameState};

/// Build a two-player lands-only game: `lib` basic lands each, two seeded `RandomAgent`s.
fn lands_only_game(lib: usize, seed: u64) -> Engine {
    let mut state = GameState::new(2, seed);
    // A simple two-name "deck" of basic lands per seat (names are just data; the core
    // never matches on them).
    let names = ["Forest", "Mountain"];
    for seat in 0..2u32 {
        for i in 0..lib {
            state.add_card(
                PlayerId(seat),
                Characteristics::basic_land(names[i % names.len()]),
                Zone::Library,
            );
        }
    }
    let agents: Vec<Box<dyn Agent>> = vec![
        Box::new(RandomAgent::new(seed ^ 0xA11CE)),
        Box::new(RandomAgent::new(seed ^ 0xB0B)),
    ];
    Engine::new(state, agents)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let seed: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(42);
    let lib: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);

    println!("mtgenv — headless lands-only self-play (milestone 2)");
    println!("  seed={seed}, library={lib} basic lands per player\n");

    let mut engine = lands_only_game(lib, seed);
    engine.record_events(true);
    let winner = engine.run_game();

    println!("recorded {} public events.\n", engine.event_log.len());

    let s = &engine.state;
    println!("game over after {} turns.", s.turn_number);
    match winner {
        Some(p) => println!("  winner: {p:?}"),
        None => println!("  result: draw / no survivor"),
    }
    for p in &s.players {
        println!(
            "  {:?}: life={} library={} hand={} battlefield={} graveyard={} lost={}",
            p.id,
            p.life,
            p.library.len(),
            p.hand.len(),
            p.battlefield.len(),
            p.graveyard.len(),
            p.has_lost,
        );
    }
}
