//! Wilds of Eldraine (`woe`) — cards whose first printing is WOE (in the SoS pool as reprints).

pub mod monstrous_rage;
pub mod quick_study;
pub mod royal_treatment;

pub fn register(db: &mut super::CardDb) {
    quick_study::register(db);
    monstrous_rage::register(db);
    royal_treatment::register(db);
}
