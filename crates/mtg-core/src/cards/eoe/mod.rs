//! EOE (Edge of Eternities) — first-printing-set folder.

use crate::cards::CardDb;

pub mod icetill_explorer;

pub fn register(db: &mut CardDb) {
    icetill_explorer::register(db);
}
