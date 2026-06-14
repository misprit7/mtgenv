//! PLS (Planeshift) — first-printing-set folder.

use crate::cards::CardDb;

pub mod flametongue_kavu;

pub fn register(db: &mut CardDb) {
    flametongue_kavu::register(db);
}
