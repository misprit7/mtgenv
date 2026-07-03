//! RAV (Ravnica: City of Guilds) — first-printing-set folder.

use crate::cards::CardDb;

pub mod last_gasp;
pub mod temple_garden;

pub fn register(db: &mut CardDb) {
    last_gasp::register(db);
    temple_garden::register(db);
}
