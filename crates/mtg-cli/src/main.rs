//! Thin CLI entrypoint for the headless engine: a self-play harness
//! (ENGINE_PLAN milestone 3). Two `RandomAgent`s play the starter R/G demo deck — playing
//! lands, casting vanilla creatures + burn, attacking — until someone reaches 0 life or
//! decks out. Usage: `mtg-cli [seed]`.

use std::env;

use mtg_core::agent::{Agent, RandomAgent};
use mtg_core::cards;
use mtg_core::priority::Engine;

fn main() {
    let args: Vec<String> = env::args().collect();
    let seed: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(42);

    println!("mtgenv — headless self-play (milestone 3)");
    println!("  seed={seed}, two R/G demo decks (lands + vanilla creatures + Shock)\n");

    let state = cards::two_player_demo_game(seed);
    let agents: Vec<Box<dyn Agent>> = vec![
        Box::new(RandomAgent::new(seed ^ 0xA11CE)),
        Box::new(RandomAgent::new(seed ^ 0xB0B)),
    ];
    let mut engine = Engine::new(state, agents);
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
