//! Thin CLI entrypoint for the headless engine. A real sim / self-play harness lands
//! once the turn engine, stack and combat are implemented (ENGINE_PLAN milestones 2–3).

use mtg_core::ids::PlayerId;
use mtg_core::rng::Rng;

fn main() {
    // Placeholder: exercise mtg-core so the dependency is real, and show determinism.
    let mut rng = Rng::new(42);
    let seat = PlayerId(0);
    println!("mtgenv — headless mtg-core engine.");
    println!("  seat {seat:?}, rng sample {}", rng.next_u64());
    println!("  (scaffold: turn/priority/stack/whiteboard/combat are stubs — see ENGINE_PLAN)");
}
