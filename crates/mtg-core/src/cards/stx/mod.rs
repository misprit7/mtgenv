//! STX (Strixhaven: School of Mages) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod fracture;

pub fn register(db: &mut CardDb) {
    fracture::register(db);
}
