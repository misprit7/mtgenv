//! KTK (Khans of Tarkir) — first-printing-set folder.

use crate::cards::CardDb;

pub mod disdainful_stroke;
pub mod hardened_scales;

pub fn register(db: &mut CardDb) {
    disdainful_stroke::register(db);
    hardened_scales::register(db);
}
