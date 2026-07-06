//! BLB (Bloomburrow) — first-printing-set folder.

use crate::cards::CardDb;

pub mod hop_to_it;
pub mod keen_eyed_curator;

pub fn register(db: &mut CardDb) {
    hop_to_it::register(db);
    keen_eyed_curator::register(db);
}
