//! BRO (The Brothers' War) — first-printing-set folder.

use crate::cards::CardDb;

pub mod bushwhack;

pub fn register(db: &mut CardDb) {
    bushwhack::register(db);
}
