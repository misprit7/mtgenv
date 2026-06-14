//! SOS (Edge of … "sos") — first-printing-set folder.

use crate::cards::CardDb;

pub mod erode;

pub fn register(db: &mut CardDb) {
    erode::register(db);
}
