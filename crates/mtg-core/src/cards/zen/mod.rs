//! ZEN (Zendikar) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod spell_pierce;

pub fn register(db: &mut CardDb) {
    spell_pierce::register(db);
}
