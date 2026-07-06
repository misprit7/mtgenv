//! Cards first printed in TDM (Tarkir: Dragonstorm).

use crate::cards::CardDb;

pub mod duty_beyond_death;
pub mod surrak_elusive_hunter;

pub fn register(db: &mut CardDb) {
    duty_beyond_death::register(db);
    surrak_elusive_hunter::register(db);
}
