//! M13 (Magic 2013) — first-printing-set folder.

use crate::cards::CardDb;

pub mod murder;

pub fn register(db: &mut CardDb) {
    murder::register(db);
}
