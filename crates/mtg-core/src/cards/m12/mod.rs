//! M12 (Magic 2012) — first-printing-set folder.

use crate::cards::CardDb;

pub mod gladecover_scout;

pub fn register(db: &mut CardDb) {
    gladecover_scout::register(db);
}
