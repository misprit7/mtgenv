//! ALA (Shards of Alara) — first-printing-set folder.

use crate::cards::CardDb;

pub mod ad_nauseam;
pub mod elvish_visionary;

pub fn register(db: &mut CardDb) {
    ad_nauseam::register(db);
    elvish_visionary::register(db);
}
