//! ISD (Innistrad) — first-printing-set folder.

use crate::cards::CardDb;

pub mod typhoid_rats;

pub fn register(db: &mut CardDb) {
    typhoid_rats::register(db);
}
