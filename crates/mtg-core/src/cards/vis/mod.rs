//! VIS (Visions) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod vampiric_tutor;

pub fn register(db: &mut CardDb) {
    vampiric_tutor::register(db);
}
