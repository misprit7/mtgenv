//! EOE (Edge of Eternities) — first-printing-set folder.

use crate::cards::CardDb;

pub mod dyadrine_synthesis_amalgam;
pub mod icetill_explorer;
pub mod mightform_harmonizer;

pub fn register(db: &mut CardDb) {
    icetill_explorer::register(db);
    mightform_harmonizer::register(db);
    dyadrine_synthesis_amalgam::register(db);
}
