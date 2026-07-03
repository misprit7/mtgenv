//! SOS (Edge of … "sos") — first-printing-set folder.

use crate::cards::CardDb;

pub mod chase_inspiration;
pub mod erode;
pub mod grapple_with_death;
pub mod interjection;
pub mod oracles_restoration;
pub mod rearing_embermare;
pub mod shopkeepers_bane;
pub mod sneering_shadewriter;
pub mod wander_off;

pub fn register(db: &mut CardDb) {
    chase_inspiration::register(db);
    erode::register(db);
    grapple_with_death::register(db);
    interjection::register(db);
    oracles_restoration::register(db);
    rearing_embermare::register(db);
    shopkeepers_bane::register(db);
    sneering_shadewriter::register(db);
    wander_off::register(db);
}
