//! RTR (Return to Ravnica) — first-printing-set folder.

use crate::cards::CardDb;

pub mod cyclonic_rift;
pub mod fencing_ace;

pub fn register(db: &mut CardDb) {
    cyclonic_rift::register(db);
    fencing_ace::register(db);
}
