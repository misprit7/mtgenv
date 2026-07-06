//! ODY (Odyssey) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod zombify;

pub fn register(db: &mut CardDb) {
    zombify::register(db);
}
