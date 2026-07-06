//! BLB (Bloomburrow) — first-printing-set folder.

use crate::cards::CardDb;

pub mod hop_to_it;
pub mod keen_eyed_curator;
pub mod repel_calamity;
pub mod stargaze;

pub fn register(db: &mut CardDb) {
    hop_to_it::register(db);
    keen_eyed_curator::register(db);
    repel_calamity::register(db);
    stargaze::register(db);
}
