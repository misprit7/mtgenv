//! LTR (The Lord of the Rings: Tales of Middle-earth) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod reprieve;

pub fn register(db: &mut CardDb) {
    reprieve::register(db);
}
