//! KTK (Khans of Tarkir) — first-printing-set folder.

use crate::cards::CardDb;

pub mod deflecting_palm;
pub mod disdainful_stroke;
pub mod hardened_scales;

pub fn register(db: &mut CardDb) {
    deflecting_palm::register(db);
    disdainful_stroke::register(db);
    hardened_scales::register(db);
}
