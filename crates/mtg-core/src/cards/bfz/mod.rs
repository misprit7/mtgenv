//! BFZ (Battle for Zendikar) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod bring_to_light;

pub fn register(db: &mut CardDb) {
    bring_to_light::register(db);
}
