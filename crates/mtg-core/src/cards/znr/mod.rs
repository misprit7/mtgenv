//! ZNR (Zendikar Rising) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod feed_the_swarm;

pub fn register(db: &mut CardDb) {
    feed_the_swarm::register(db);
}
