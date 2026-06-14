//! BLB (Bloomburrow) — first-printing-set folder.

use crate::cards::CardDb;

pub mod keen_eyed_curator;

pub fn register(db: &mut CardDb) {
    keen_eyed_curator::register(db);
}
