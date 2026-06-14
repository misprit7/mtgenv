//! USG (Urza's Saga) — first-printing-set folder.

use crate::cards::CardDb;

pub mod argothian_swine;
pub mod glorious_anthem;

pub fn register(db: &mut CardDb) {
    argothian_swine::register(db);
    glorious_anthem::register(db);
}
