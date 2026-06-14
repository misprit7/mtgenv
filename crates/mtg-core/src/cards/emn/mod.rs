//! EMN (Eldritch Moon) — first-printing-set folder.

use crate::cards::CardDb;

pub mod exultant_cultist;

pub fn register(db: &mut CardDb) {
    exultant_cultist::register(db);
}
