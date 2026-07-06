//! FRF (Fate Reforged) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod shamanic_revelation;

pub fn register(db: &mut CardDb) {
    shamanic_revelation::register(db);
}
