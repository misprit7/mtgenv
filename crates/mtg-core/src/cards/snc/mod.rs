//! SNC (Streets of New Capenna) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod big_score;

pub fn register(db: &mut CardDb) {
    big_score::register(db);
}
