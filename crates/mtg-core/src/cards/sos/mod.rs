//! SOS (Edge of … "sos") — first-printing-set folder.

use crate::cards::CardDb;

pub mod erode;
pub mod grapple_with_death;
pub mod interjection;
pub mod rearing_embermare;
pub mod wander_off;

pub fn register(db: &mut CardDb) {
    erode::register(db);
    grapple_with_death::register(db);
    interjection::register(db);
    rearing_embermare::register(db);
    wander_off::register(db);
}
