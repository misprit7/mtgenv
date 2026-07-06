//! ULG (Urza's Legacy) — first-printing-set folder.

use crate::cards::CardDb;

pub mod crop_rotation;
pub mod levitation;

pub fn register(db: &mut CardDb) {
    crop_rotation::register(db);
    levitation::register(db);
}
