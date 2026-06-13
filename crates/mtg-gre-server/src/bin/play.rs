//! M1 binary — play a lands-only game **at the terminal** against a `RandomAgent`.
//!
//! Run: `cargo run -p mtg-gre-server --bin mtg-play [-- <seed>]`
//!
//! Proves "a human is just another Agent": you (Player 0) are a [`HumanAgent`] and your opponent
//! (Player 1) is mtg-core's [`RandomAgent`] — both behind the one decision boundary.

use mtg_core::agent::{Agent, RandomAgent};
use mtg_core::ids::PlayerId;
use mtg_gre_server::driver;
use mtg_gre_server::human::HumanAgent;

fn main() {
    let seed: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);

    println!("mtgenv — terminal play (lands-only demo).");
    println!("You are Player 0 (HumanAgent) vs Player 1 (RandomAgent). Seed {seed}.");
    println!("At each decision: type an option index, or press Enter / 'p' to pass.\n");

    let agents: Vec<Box<dyn Agent>> = vec![
        Box::new(HumanAgent::new(PlayerId(0))),
        Box::new(RandomAgent::new(seed)),
    ];
    let outcome = driver::run_lands_game(agents, seed);

    println!("\n═══════════════ GAME OVER ═══════════════");
    match outcome.winner {
        Some(p) => println!("Winner: Player {} (after {} turns)", p.0, outcome.turns),
        None => println!("Draw (after {} turns)", outcome.turns),
    }
}
