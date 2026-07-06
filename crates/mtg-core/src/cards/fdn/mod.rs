//! FDN (Foundations) — first-printing-set folder.

use crate::cards::CardDb;

pub mod bulk_up;
pub mod mossborn_hydra;

pub fn register(db: &mut CardDb) {
    bulk_up::register(db);
    mossborn_hydra::register(db);
}
