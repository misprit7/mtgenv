//! POR (Portal) — first-printing-set folder.

use crate::cards::CardDb;

pub mod raging_goblin;

pub fn register(db: &mut CardDb) {
    raging_goblin::register(db);
}
