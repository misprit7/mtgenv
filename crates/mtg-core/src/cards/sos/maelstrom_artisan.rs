//! Maelstrom Artisan // Rocket Volley — `{1}{R}{R}` Creature — Minotaur Sorcerer 3/2 // `{1}{R}` Sorcery
//! (first printed SOS). A **Prepare** DFC (enters prepared).
//!
//! Front: "This creature enters prepared."  Back (Rocket Volley): "Destroy target nonbasic land."
//!
//! **Fully implemented** — enters-prepared via [`helpers::enters_prepared`]; the back destroys a target
//! nonbasic land (a `Permanent` target filtered `Land && !Basic`).

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{CreatureType, Supertype};

pub const MAELSTROM_ARTISAN: u32 = 386;
pub const ROCKET_VOLLEY: u32 = 9713;

pub fn register(db: &mut CardDb) {
    let rocket_volley = Effect::Destroy {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Permanent(CardFilter::All(vec![
                CardFilter::HasCardType(CardType::Land),
                CardFilter::Not(Box::new(CardFilter::Supertype(Supertype::Basic))),
            ])),
            min: 1,
            max: 1,
            distinct: true,
        }),
    };
    db.insert(
        spell(ROCKET_VOLLEY, "Rocket Volley", CardType::Sorcery, Color::Red, mana_cost(1, &[(Color::Red, 1)]), rocket_volley)
            .with_text("Destroy target nonbasic land."),
    );
    let mut front = creature(
        MAELSTROM_ARTISAN,
        "Maelstrom Artisan",
        &[CreatureType::Minotaur, CreatureType::Sorcerer],
        Color::Red,
        mana_cost(1, &[(Color::Red, 2)]),
        3,
        2,
        helpers::enters_prepared(ROCKET_VOLLEY),
    );
    front.text = "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Rocket Volley {1}{R} Sorcery — Destroy target nonbasic land.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::ability::{Ability, EventPattern};
    use expect_test::expect;

    #[test]
    fn maelstrom_artisan_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let f = db.get(MAELSTROM_ARTISAN).unwrap();
        assert!(matches!(f.abilities[0], Ability::Prepare { spell: ROCKET_VOLLEY }));
        assert!(matches!(f.abilities[1], Ability::Triggered { event: EventPattern::SelfEnters, .. }));
        expect![[r#"
            Destroy {
                what: Target(
                    TargetSpec {
                        kind: Permanent(
                            All(
                                [
                                    HasCardType(
                                        Land,
                                    ),
                                    Not(
                                        Supertype(
                                            Basic,
                                        ),
                                    ),
                                ],
                            ),
                        ),
                        min: 1,
                        max: 1,
                        distinct: true,
                    },
                ),
            }"#]]
        .assert_eq(&format!("{:#?}", db.get(ROCKET_VOLLEY).unwrap().spell_effect().unwrap()));
    }
}
