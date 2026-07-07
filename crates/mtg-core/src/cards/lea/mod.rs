//! LEA (Limited Edition Alpha) — first-printing-set folder.

use crate::cards::CardDb;

pub mod armageddon;
pub mod berserk;
pub mod elvish_archers;
pub mod giant_growth;
pub mod grizzly_bears;
pub mod hill_giant;
pub mod lightning_bolt;
pub mod llanowar_elves;
pub mod wall_of_stone;

pub fn register(db: &mut CardDb) {
    armageddon::register(db);
    berserk::register(db);
    elvish_archers::register(db);
    giant_growth::register(db);
    grizzly_bears::register(db);
    hill_giant::register(db);
    lightning_bolt::register(db);
    llanowar_elves::register(db);
    wall_of_stone::register(db);
}
