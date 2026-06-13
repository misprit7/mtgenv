//! LEA (Limited Edition Alpha) — first-printing-set folder.

use crate::cards::CardDb;

pub mod llanowar_elves;

pub fn register(db: &mut CardDb) {
    llanowar_elves::register(db);
}
