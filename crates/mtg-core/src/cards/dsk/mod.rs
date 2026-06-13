//! DSK (Duskmourn: House of Horror) — first-printing-set folder.

use crate::cards::CardDb;

pub mod hushwood_verge;

pub fn register(db: &mut CardDb) {
    hushwood_verge::register(db);
}
