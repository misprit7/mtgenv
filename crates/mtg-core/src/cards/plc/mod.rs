//! PLC (Planar Chaos) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod pongify;

pub fn register(db: &mut CardDb) {
    pongify::register(db);
}
