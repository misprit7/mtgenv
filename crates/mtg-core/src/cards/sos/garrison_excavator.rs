//! Garrison Excavator — `{3}{R}` Creature — Orc Sorcerer 3/4 (first printed SOS).
//!
//! Oracle: "Menace / Whenever one or more cards leave your graveyard, create a 2/2 red and white
//! Spirit creature token."
//!
//! **Fully implemented** — printed Menace + a `CardsLeaveYourGraveyard` trigger making a Spirit token.

use crate::basics::Color;
use crate::cards::helpers::spirit_token;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const GARRISON_EXCAVATOR: u32 = 304;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        GARRISON_EXCAVATOR,
        "Garrison Excavator",
        &[CreatureType::Orc, CreatureType::Sorcerer],
        Color::Red,
        mana_cost(3, &[(Color::Red, 1)]),
        3,
        4,
        vec![Ability::Triggered {
            event: EventPattern::CardsLeaveYourGraveyard,
            condition: None,
            intervening_if: false,
            effect: Effect::CreateToken { spec: spirit_token(), count: ValueExpr::Fixed(1), controller: PlayerRef::Controller },
        }],
    );
    def.chars.keywords = vec![Keyword::Menace];
    def.text = "Menace\nWhenever one or more cards leave your graveyard, create a 2/2 red and white Spirit creature token.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garrison_excavator_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(GARRISON_EXCAVATOR).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Menace]);
        assert!(matches!(&def.abilities[0], Ability::Triggered { event: EventPattern::CardsLeaveYourGraveyard, .. }));
        assert!(def.fully_implemented);
    }
}
