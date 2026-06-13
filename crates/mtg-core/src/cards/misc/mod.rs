//! Misc / starter cards — the prototype pool that predates the real per-set card push. These
//! have no meaningful first-printing set (they're test/prototype bodies exercising engine
//! milestones M3–M5 + the #14 breadth), so they live here grouped by mechanic rather than in a
//! `<setcode>/` folder. Real cards go in per-set folders (see the card-push spec).
//!
//! Each submodule exposes `register(&mut CardDb)`; [`register`] aggregates them all. The card
//! *builders* (`creature`/`spell`/`aura`/…) and id constants live in the parent (`crate::cards`).

use crate::cards::CardDb;

pub mod basics;
pub mod enchantments;
pub mod planeswalkers;
pub mod spells;
pub mod triggers;
pub mod vanilla;

/// Insert every misc/starter card into `db`.
pub fn register(db: &mut CardDb) {
    basics::register(db);
    spells::register(db);
    vanilla::register(db);
    triggers::register(db);
    enchantments::register(db);
    planeswalkers::register(db);
}
