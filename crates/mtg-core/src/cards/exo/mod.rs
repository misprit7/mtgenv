//! EXO (Exodus) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod culling_the_weak;

pub fn register(db: &mut CardDb) {
    culling_the_weak::register(db);
}
