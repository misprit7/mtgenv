//! Nemesis (`nem`) — first-printing-set folder for `soa` bonus-sheet reprints.

use crate::cards::CardDb;

pub mod daze;

pub fn register(db: &mut CardDb) {
    daze::register(db);
}
