//! FIN (Final Fantasy) — first-printing-set folder.

use crate::cards::CardDb;

pub mod sazhs_chocobo;

pub fn register(db: &mut CardDb) {
    sazhs_chocobo::register(db);
}
