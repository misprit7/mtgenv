//! MRD (Mirrodin) — first-printing-set folder.

use crate::cards::CardDb;

pub mod bonesplitter;

pub fn register(db: &mut CardDb) {
    bonesplitter::register(db);
}
