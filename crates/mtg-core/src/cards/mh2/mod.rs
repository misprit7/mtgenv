//! MH2 (Modern Horizons 2) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod prismatic_ending;

pub fn register(db: &mut CardDb) {
    prismatic_ending::register(db);
}
