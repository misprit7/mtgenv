//! M15 (Magic 2015) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod return_to_the_ranks;

pub fn register(db: &mut CardDb) {
    return_to_the_ranks::register(db);
}
