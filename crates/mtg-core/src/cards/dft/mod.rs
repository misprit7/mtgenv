//! DFT (Aetherdrift) — first-printing-set folder.

use crate::cards::CardDb;

pub mod lumbering_worldwagon;

pub fn register(db: &mut CardDb) {
    lumbering_worldwagon::register(db);
}
