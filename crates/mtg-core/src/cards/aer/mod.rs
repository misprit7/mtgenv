//! AER (Aether Revolt) — first-printing-set folder.

use crate::cards::CardDb;

pub mod alley_strangler;

pub fn register(db: &mut CardDb) {
    alley_strangler::register(db);
}
