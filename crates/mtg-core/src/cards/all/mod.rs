//! Alliances (`all`) — first-printing-set folder for `soa` bonus-sheet reprints.

use crate::cards::CardDb;

pub mod force_of_will;

pub fn register(db: &mut CardDb) {
    force_of_will::register(db);
}
