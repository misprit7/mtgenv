//! ULG (Urza's Legacy) — first-printing-set folder.

use crate::cards::CardDb;

pub mod levitation;

pub fn register(db: &mut CardDb) {
    levitation::register(db);
}
