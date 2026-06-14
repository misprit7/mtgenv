//! RTR (Return to Ravnica) — first-printing-set folder.

use crate::cards::CardDb;

pub mod fencing_ace;

pub fn register(db: &mut CardDb) {
    fencing_ace::register(db);
}
