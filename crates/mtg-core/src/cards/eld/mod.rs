//! ELD (Throne of Eldraine) — first-printing-set folder.

use crate::cards::CardDb;

pub mod fabled_passage;

pub fn register(db: &mut CardDb) {
    fabled_passage::register(db);
}
