//! SOS (Edge of … "sos") — first-printing-set folder.

use crate::cards::CardDb;

pub mod bogwater_lumaret;
pub mod chase_inspiration;
pub mod environmental_scientist;
pub mod erode;
pub mod grapple_with_death;
pub mod interjection;
pub mod mindful_biomancer;
pub mod noxious_newt;
pub mod oracles_restoration;
pub mod rearing_embermare;
pub mod shopkeepers_bane;
pub mod silverquill_charm;
pub mod sneering_shadewriter;
pub mod traumatic_critique;
pub mod vibrant_outburst;
pub mod wander_off;
pub mod zealous_lorecaster;

pub fn register(db: &mut CardDb) {
    bogwater_lumaret::register(db);
    chase_inspiration::register(db);
    environmental_scientist::register(db);
    erode::register(db);
    grapple_with_death::register(db);
    interjection::register(db);
    mindful_biomancer::register(db);
    noxious_newt::register(db);
    oracles_restoration::register(db);
    rearing_embermare::register(db);
    shopkeepers_bane::register(db);
    silverquill_charm::register(db);
    sneering_shadewriter::register(db);
    traumatic_critique::register(db);
    vibrant_outburst::register(db);
    wander_off::register(db);
    zealous_lorecaster::register(db);
}
