//! MKM (Murders at Karlov Manor) — first-printing-set folder.

use crate::cards::CardDb;

pub mod escape_tunnel;
pub mod pick_your_poison;

pub fn register(db: &mut CardDb) {
    escape_tunnel::register(db);
    pick_your_poison::register(db);
}
