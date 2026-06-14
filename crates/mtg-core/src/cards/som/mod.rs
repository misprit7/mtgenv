//! SOM (Scars of Mirrodin) — first-printing-set folder.

use crate::cards::CardDb;

pub mod darksteel_myr;

pub fn register(db: &mut CardDb) {
    darksteel_myr::register(db);
}
