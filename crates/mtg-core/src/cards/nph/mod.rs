//! NPH (New Phyrexia) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod dismember;
pub mod triumph_of_the_hordes;

pub fn register(db: &mut CardDb) {
    dismember::register(db);
    triumph_of_the_hordes::register(db);
}
