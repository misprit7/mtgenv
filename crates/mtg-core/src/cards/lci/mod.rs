//! LCI (The Lost Caverns of Ixalan) — first-printing-set folder. Holds `soa` bonus-sheet reprints first printed here.

use crate::cards::CardDb;

pub mod bitter_triumph;
pub mod helping_hand;

pub fn register(db: &mut CardDb) {
    bitter_triumph::register(db);
    helping_hand::register(db);
}
