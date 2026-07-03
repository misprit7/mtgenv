//! SOS (Edge of … "sos") — first-printing-set folder.

use crate::cards::CardDb;

pub mod bogwater_lumaret;
pub mod additive_evolution;
pub mod burrog_banemaker;
pub mod chase_inspiration;
pub mod chelonian_tackle;
pub mod eager_glyphmage;
pub mod environmental_scientist;
pub mod erode;
pub mod grapple_with_death;
pub mod harsh_annotation;
pub mod interjection;
pub mod masterful_flourish;
pub mod mindful_biomancer;
pub mod noxious_newt;
pub mod oracles_restoration;
pub mod proctors_gaze;
pub mod rapturous_moment;
pub mod rearing_embermare;
pub mod shattered_acolyte;
pub mod shopkeepers_bane;
pub mod silverquill_charm;
pub mod sneering_shadewriter;
pub mod stadium_tidalmage;
pub mod startled_relic_sloth;
pub mod traumatic_critique;
pub mod vibrant_outburst;
pub mod wander_off;
pub mod zealous_lorecaster;

pub fn register(db: &mut CardDb) {
    additive_evolution::register(db);
    bogwater_lumaret::register(db);
    burrog_banemaker::register(db);
    chase_inspiration::register(db);
    chelonian_tackle::register(db);
    eager_glyphmage::register(db);
    environmental_scientist::register(db);
    erode::register(db);
    grapple_with_death::register(db);
    harsh_annotation::register(db);
    interjection::register(db);
    masterful_flourish::register(db);
    mindful_biomancer::register(db);
    noxious_newt::register(db);
    oracles_restoration::register(db);
    proctors_gaze::register(db);
    rapturous_moment::register(db);
    rearing_embermare::register(db);
    shattered_acolyte::register(db);
    shopkeepers_bane::register(db);
    silverquill_charm::register(db);
    sneering_shadewriter::register(db);
    stadium_tidalmage::register(db);
    startled_relic_sloth::register(db);
    traumatic_critique::register(db);
    vibrant_outburst::register(db);
    wander_off::register(db);
    zealous_lorecaster::register(db);
}
