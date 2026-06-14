//! KTK (Khans of Tarkir) — first-printing-set folder.

use crate::cards::CardDb;

pub mod hardened_scales;

pub fn register(db: &mut CardDb) {
    hardened_scales::register(db);
}
