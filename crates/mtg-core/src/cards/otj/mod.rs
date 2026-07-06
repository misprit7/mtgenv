//! OTJ (Outlaws of Thunder Junction) — first-printing-set folder for `soa` bonus-sheet reprints.

use crate::cards::CardDb;

pub mod requisition_raid;
pub mod return_the_favor;

pub fn register(db: &mut CardDb) {
    requisition_raid::register(db);
    return_the_favor::register(db);
}
