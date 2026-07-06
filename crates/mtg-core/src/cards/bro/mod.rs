//! BRO (The Brothers' War) — first-printing-set folder.

use crate::cards::CardDb;

pub mod awaken_the_woods;
pub mod brotherhoods_end;
pub mod bushwhack;

pub fn register(db: &mut CardDb) {
    awaken_the_woods::register(db);
    brotherhoods_end::register(db);
    bushwhack::register(db);
}
