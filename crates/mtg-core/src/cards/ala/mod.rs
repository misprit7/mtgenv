//! ALA (Shards of Alara) — first-printing-set folder.

use crate::cards::CardDb;

pub mod elvish_visionary;

pub fn register(db: &mut CardDb) {
    elvish_visionary::register(db);
}
