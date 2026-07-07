//! DFT (Aetherdrift) — first-printing-set folder.

use crate::cards::CardDb;

pub mod locust_spray;
pub mod lumbering_worldwagon;
pub mod stock_up;

pub fn register(db: &mut CardDb) {
    lumbering_worldwagon::register(db);
    stock_up::register(db);
    locust_spray::register(db);
}
