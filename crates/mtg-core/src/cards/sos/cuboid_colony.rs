//! Cuboid Colony — `{G}{U}` Creature — Insect 1/1 (first printed SOS).
//!
//! Oracle: "Flash / Flying, trample / Increment (Whenever you cast a spell, if the amount of mana
//! you spent is greater than this creature's power or toughness, put a +1/+1 counter on this
//! creature.)"
//!
//! **Fully implemented** — printed Flash + Flying + Trample + the shared Increment cast-trigger.
//! Multicolored (G/U).

use crate::basics::Color;
use crate::cards::helpers::increment_ability;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const CUBOID_COLONY: u32 = 265;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        CUBOID_COLONY,
        "Cuboid Colony",
        &[CreatureType::Insect],
        Color::Green,
        mana_cost(0, &[(Color::Green, 1), (Color::Blue, 1)]),
        1,
        1,
        vec![increment_ability()],
    );
    def.chars.colors = vec![Color::Green, Color::Blue];
    def.chars.keywords = vec![Keyword::Flash, Keyword::Flying, Keyword::Trample];
    def.text = "Flash\nFlying, trample\nIncrement (Whenever you cast a spell, if the amount of mana you spent is greater than this creature's power or toughness, put a +1/+1 counter on this creature.)".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuboid_colony_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(CUBOID_COLONY).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Green, Color::Blue]);
        assert_eq!(def.chars.keywords, vec![Keyword::Flash, Keyword::Flying, Keyword::Trample]);
        assert!(def.fully_implemented);
        assert_eq!(def.abilities.len(), 1, "the Increment trigger");
    }
}
