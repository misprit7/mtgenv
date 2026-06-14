//! RAV (Ravnica: City of Guilds) — first-printing-set folder.

use crate::cards::CardDb;

pub mod temple_garden;

pub fn register(db: &mut CardDb) {
    temple_garden::register(db);
}
