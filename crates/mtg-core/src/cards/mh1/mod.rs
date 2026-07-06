//! MH1 (Modern Horizons) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod winds_of_abandon;

pub fn register(db: &mut CardDb) {
    winds_of_abandon::register(db);
}
