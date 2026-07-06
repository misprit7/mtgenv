//! STX (Strixhaven: School of Mages) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod crackle_with_power;
pub mod expressive_iteration;
pub mod fracture;

pub fn register(db: &mut CardDb) {
    crackle_with_power::register(db);
    expressive_iteration::register(db);
    fracture::register(db);
}
