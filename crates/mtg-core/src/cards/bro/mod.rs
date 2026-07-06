//! BRO (The Brothers' War) — first-printing-set folder.

use crate::cards::CardDb;

pub mod brotherhoods_end;
pub mod bushwhack;

pub fn register(db: &mut CardDb) {
    brotherhoods_end::register(db);
    bushwhack::register(db);
}
