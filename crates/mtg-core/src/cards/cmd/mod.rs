//! CMD (Magic: The Gathering—Commander) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod flusterstorm;

pub fn register(db: &mut CardDb) {
    flusterstorm::register(db);
}
