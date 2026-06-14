//! M10 (Magic 2010) — first-printing-set folder.

use crate::cards::CardDb;

pub mod child_of_night;
pub mod divination;

pub fn register(db: &mut CardDb) {
    child_of_night::register(db);
    divination::register(db);
}
