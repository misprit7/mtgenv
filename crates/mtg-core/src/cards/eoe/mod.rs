//! EOE (Edge of Eternities) — first-printing-set folder.

use crate::cards::CardDb;

pub mod icetill_explorer;
pub mod mightform_harmonizer;

pub fn register(db: &mut CardDb) {
    icetill_explorer::register(db);
    mightform_harmonizer::register(db);
}
