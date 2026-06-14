//! MGB (Multiverse Gift Box) — first-printing-set folder.

use crate::cards::CardDb;

pub mod king_cheetah;

pub fn register(db: &mut CardDb) {
    king_cheetah::register(db);
}
