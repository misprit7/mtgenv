//! LEA (Limited Edition Alpha) — first-printing-set folder.

use crate::cards::CardDb;

pub mod elvish_archers;
pub mod grizzly_bears;
pub mod hill_giant;
pub mod lightning_bolt;
pub mod llanowar_elves;
pub mod wall_of_stone;

pub fn register(db: &mut CardDb) {
    elvish_archers::register(db);
    grizzly_bears::register(db);
    hill_giant::register(db);
    lightning_bolt::register(db);
    llanowar_elves::register(db);
    wall_of_stone::register(db);
}
