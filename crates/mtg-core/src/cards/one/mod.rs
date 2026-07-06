//! ONE (Phyrexia: All Will Be One) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod sheoldreds_edict;

pub fn register(db: &mut CardDb) {
    sheoldreds_edict::register(db);
}
