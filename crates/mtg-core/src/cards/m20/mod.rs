//! M20 (Core Set 2020) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod veil_of_summer;

pub fn register(db: &mut CardDb) {
    veil_of_summer::register(db);
}
