//! STH (Stronghold) — first-printing-set folder.

use crate::cards::CardDb;

pub mod shock;

pub fn register(db: &mut CardDb) {
    shock::register(db);
}
