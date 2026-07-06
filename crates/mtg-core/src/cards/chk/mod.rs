//! CHK (Champions of Kamigawa) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod glimpse_of_nature;

pub fn register(db: &mut CardDb) {
    glimpse_of_nature::register(db);
}
