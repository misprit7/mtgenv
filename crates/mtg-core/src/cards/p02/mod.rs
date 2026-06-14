//! P02 (Portal Second Age) — first-printing-set folder.

use crate::cards::CardDb;

pub mod alaborn_grenadier;

pub fn register(db: &mut CardDb) {
    alaborn_grenadier::register(db);
}
