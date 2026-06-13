//! Thin CLI entrypoint for the headless engine: a self-play harness
//! (ENGINE_PLAN milestone 3). Two `RandomAgent`s play preset decks — playing lands, casting
//! creatures + burn, attacking — until someone reaches 0 life or decks out.
//!
//! Usage: `mtg-cli [seed] [deckA] [deckB]`  where deck ∈ {demo, burn, bears} (default demo).
//! e.g. `mtg-cli 1 burn bears`.

use std::env;

use mtg_core::agent::{Agent, RandomAgent};
use mtg_core::cards;
use mtg_core::priority::Engine;

fn main() {
    let args: Vec<String> = env::args().collect();
    let seed: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(42);
    let deck_a = args.get(2).map(String::as_str).unwrap_or("demo");
    let deck_b = args.get(3).map(String::as_str).unwrap_or("demo");
    let da = cards::preset_deck(deck_a).unwrap_or_else(cards::demo_deck);
    let db = cards::preset_deck(deck_b).unwrap_or_else(cards::demo_deck);

    println!("mtgenv — headless self-play (milestone 3)");
    println!("  seed={seed}, P0={deck_a} vs P1={deck_b}\n");

    let state = cards::build_game(seed, &[&da, &db]);
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
