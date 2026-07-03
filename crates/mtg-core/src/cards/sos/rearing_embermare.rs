//! Rearing Embermare — `{4}{R}` Creature — Horse Beast 4/5 (first printed SOS).
//!
//! Oracle: "Reach, haste"
//!
//! **Fully implemented** — a french-vanilla creature: two printed evergreen keywords (CR 702.9
//! Reach, 702.10 Haste) and nothing else. Carried as `chars.keywords`; no abilities, no effect.

use crate::basics::Color;
use crate::cards::{kw_creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const REARING_EMBERMARE: u32 = 200;

pub fn register(db: &mut CardDb) {
    let mut def = kw_creature(
        REARING_EMBERMARE,
        "Rearing Embermare",
        &[CreatureType::Horse, CreatureType::Beast],
        Color::Red,
        mana_cost(4, &[(Color::Red, 1)]),
        4,
        5,
        vec![Keyword::Reach, Keyword::Haste],
    );
    def.text = "Reach, haste".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;

    #[test]
    fn rearing_embermare_is_french_vanilla() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(REARING_EMBERMARE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(def.chars.power, Some(4));
        assert_eq!(def.chars.toughness, Some(5));
        assert_eq!(def.chars.keywords, vec![Keyword::Reach, Keyword::Haste]);
        assert!(def.abilities.is_empty(), "french-vanilla: no abilities");
        assert!(def.fully_implemented);
        assert!(!def.is_mana_source());
    }
}
