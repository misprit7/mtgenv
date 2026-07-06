//! HOU (Hour of Devastation) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod abrade;

pub fn register(db: &mut CardDb) {
    abrade::register(db);
}
