//! RIX (Rivals of Ixalan) — first-printing-set folder.

use crate::cards::CardDb;

pub mod mist_cloaked_herald;

pub fn register(db: &mut CardDb) {
    mist_cloaked_herald::register(db);
}
