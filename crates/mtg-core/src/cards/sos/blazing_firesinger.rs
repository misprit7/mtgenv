//! Blazing Firesinger // Seething Song — `{2}{R}` Creature — Dwarf Bard 2/3 // `{2}{R}` Instant
//! (first printed SOS). A **Prepare** DFC (enters prepared).
//!
//! Front: "This creature enters prepared."  Back (Seething Song): "Add {R}{R}{R}{R}{R}."
//!
//! **Fully implemented** — enters-prepared via [`helpers::enters_prepared`]; the back face is a mana
//! ritual (five red). It's an instant *spell* (not a mana ability, CR 605.1a), so the prepared cast
//! is offered at instant speed and the mana is added when the copy resolves.

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::target::ManaSpec;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

pub const BLAZING_FIRESINGER: u32 = 379;
pub const SEETHING_SONG: u32 = 9706;

pub fn register(db: &mut CardDb) {
    db.insert(
        spell(
            SEETHING_SONG,
            "Seething Song",
            CardType::Instant,
            Color::Red,
            mana_cost(2, &[(Color::Red, 1)]),
            Effect::AddMana {
                who: PlayerRef::Controller,
                mana: ManaSpec {
                    produces: vec![(Color::Red, ValueExpr::Fixed(5))],
                    any_color: None,
                    restriction: None,
                },
            },
        )
        .with_text("Add {R}{R}{R}{R}{R}."),
    );
    let mut front = creature(
        BLAZING_FIRESINGER,
        "Blazing Firesinger",
        &[CreatureType::Dwarf, CreatureType::Bard],
        Color::Red,
        mana_cost(2, &[(Color::Red, 1)]),
        2,
        3,
        helpers::enters_prepared(SEETHING_SONG),
    );
    front.text = "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Seething Song {2}{R} Instant — Add {R}{R}{R}{R}{R}.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::ability::{Ability, EventPattern};
    use expect_test::expect;

    #[test]
    fn blazing_firesinger_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let f = db.get(BLAZING_FIRESINGER).unwrap();
        assert!(matches!(f.abilities[0], Ability::Prepare { spell: SEETHING_SONG }));
        assert!(matches!(f.abilities[1], Ability::Triggered { event: EventPattern::SelfEnters, .. }));
        expect![[r#"
            AddMana {
                who: Controller,
                mana: ManaSpec {
                    produces: [
                        (
                            Red,
                            Fixed(
                                5,
                            ),
                        ),
                    ],
                    any_color: None,
                    restriction: None,
                },
            }"#]]
        .assert_eq(&format!("{:#?}", db.get(SEETHING_SONG).unwrap().spell_effect().unwrap()));
    }
}
