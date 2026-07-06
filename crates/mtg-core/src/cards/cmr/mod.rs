//! CMR (Commander Legends) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod jeskas_will;

pub fn register(db: &mut CardDb) {
    jeskas_will::register(db);
}
