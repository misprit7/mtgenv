//! TMP (Tempest) — first-printing-set folder.

use crate::cards::CardDb;

pub mod natures_revolt;
pub mod root_maze;

pub fn register(db: &mut CardDb) {
    natures_revolt::register(db);
    root_maze::register(db);
}
