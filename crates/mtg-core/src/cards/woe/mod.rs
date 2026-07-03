//! Wilds of Eldraine (`woe`) — cards whose first printing is WOE (in the SoS pool as reprints).

pub mod quick_study;

pub fn register(db: &mut super::CardDb) {
    quick_study::register(db);
}
