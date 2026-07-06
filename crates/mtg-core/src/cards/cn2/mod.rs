//! CN2 (Conspiracy: Take the Crown) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod subterranean_tremors;

pub fn register(db: &mut CardDb) {
    subterranean_tremors::register(db);
}
