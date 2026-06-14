//! Cards first printed in TDM (Tarkir: Dragonstorm).

use crate::cards::CardDb;

pub mod surrak_elusive_hunter;

pub fn register(db: &mut CardDb) {
    surrak_elusive_hunter::register(db);
}
