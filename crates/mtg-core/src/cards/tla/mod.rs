//! TLA (Avatar: The Last Airbender) — first-printing-set folder.

use crate::cards::CardDb;

pub mod ba_sing_se;

pub fn register(db: &mut CardDb) {
    ba_sing_se::register(db);
}
