//! VOW (Innistrad: Crimson Vow) — first-printing-set folder. The SoS reprints here are the
//! "slow land" / check-land dual cycle.

use crate::cards::CardDb;

pub mod deathcap_glade;
pub mod dreamroot_cascade;
pub mod shattered_sanctum;
pub mod stormcarved_coast;
pub mod sundown_pass;

pub fn register(db: &mut CardDb) {
    deathcap_glade::register(db);
    dreamroot_cascade::register(db);
    shattered_sanctum::register(db);
    stormcarved_coast::register(db);
    sundown_pass::register(db);
}
