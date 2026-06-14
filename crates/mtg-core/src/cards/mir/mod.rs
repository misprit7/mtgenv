//! MIR (Mirage) — first-printing-set folder.

use crate::cards::CardDb;

pub mod pacifism;

pub fn register(db: &mut CardDb) {
    pacifism::register(db);
}
