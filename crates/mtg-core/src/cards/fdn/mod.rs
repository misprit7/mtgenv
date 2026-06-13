//! FDN (Foundations) — first-printing-set folder.

use crate::cards::CardDb;

pub mod mossborn_hydra;

pub fn register(db: &mut CardDb) {
    mossborn_hydra::register(db);
}
