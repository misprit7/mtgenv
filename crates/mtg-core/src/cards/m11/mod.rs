//! M11 (Magic 2011) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod preordain;
pub mod pyretic_ritual;

pub fn register(db: &mut CardDb) {
    preordain::register(db);
    pyretic_ritual::register(db);
}
