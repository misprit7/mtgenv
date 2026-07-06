//! SCG (Scourge) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod brain_freeze;

pub fn register(db: &mut CardDb) {
    brain_freeze::register(db);
}
