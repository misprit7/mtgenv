//! Badgermole Cub — `{1}{G}` Creature — Badger Mole 2/2 (first printed TLA, Avatar: The Last
//! Airbender).
//!
//! Oracle:
//!   When this creature enters, earthbend 1. (Target land you control becomes a 0/0 creature with
//!   haste that's still a land. Put a +1/+1 counter on it. When it dies or is exiled, return it to
//!   the battlefield tapped.)
//!   Whenever you tap a creature for mana, add an additional {G}.
//!
//! **Fully implemented** — both abilities faithful:
//! - "When this creature enters, **earthbend 1**" — a `Triggered{SelfEnters}` over
//!   `Effect::Earthbend{target: target land you control, n: 1}` (C12, fully landed incl. the
//!   "dies/exiled → return tapped" delayed trigger). The targeted land becomes a 0/0 haste
//!   land-creature with one +1/+1 counter.
//! - "Whenever you tap a creature for mana, add an additional {G}." — a `Triggered{TapCreatureForMana}`
//!   (cap 23242f2; CR 605.1b, fires per creature tapped for mana) over `Effect::AddMana{Controller, {G}}`,
//!   a no-stack triggered mana ability. So tapping any creature for mana yields an extra green.

use crate::basics::Color;
use crate::cards::helpers::earthbend;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::ManaSpec;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const BADGERMOLE_CUB: u32 = 113;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        BADGERMOLE_CUB,
        "Badgermole Cub",
        &[CreatureType::Badger, CreatureType::Mole],
        Color::Green,
        mana_cost(1, &[(Color::Green, 1)]),
        2,
        2,
        vec![
            // "When this creature enters, earthbend 1."
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: earthbend(1),
            },
            // "Whenever you tap a creature for mana, add an additional {G}." (no-stack mana trigger).
            Ability::Triggered {
                event: EventPattern::TapCreatureForMana,
                condition: None,
                intervening_if: false,
                effect: Effect::AddMana {
                    who: PlayerRef::Controller,
                    mana: ManaSpec {
                        produces: vec![(Color::Green, ValueExpr::Fixed(1))],
                        any_color: None,
                    },
                },
            },
        ],
    );
    def.text = "When this creature enters, earthbend 1. (Target land you control becomes a 0/0 creature with haste that's still a land. Put a +1/+1 counter on it. When it dies or is exiled, return it to the battlefield tapped.)\nWhenever you tap a creature for mana, add an additional {G}.".to_string();
    // Fully implemented: ETB earthbend 1 (C12) + the reflexive "tap a creature for mana → add {G}"
    // trigger (cap 23242f2). See module docs.
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;
    use crate::subtypes::Subtype;
    use expect_test::expect;

    #[test]
    fn badgermole_cub_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BADGERMOLE_CUB).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(
            def.chars.subtypes,
            vec![Subtype::Creature(CreatureType::Badger), Subtype::Creature(CreatureType::Mole)]
        );
        assert_eq!((def.chars.power, def.chars.toughness), (Some(2), Some(2)));
        // Fully implemented: ETB earthbend 1 + the reflexive "tap a creature for mana → add {G}" trigger.
        assert!(def.fully_implemented);
        // ETB earthbend trigger (targets "a land you control") + the TapCreatureForMana → add {G} trigger.
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Earthbend {
                        target: Target(
                            TargetSpec {
                                kind: Permanent(
                                    All(
                                        [
                                            HasCardType(
                                                Land,
                                            ),
                                            ControlledBy(
                                                Controller,
                                            ),
                                        ],
                                    ),
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        n: Fixed(
                            1,
                        ),
                    },
                },
                Triggered {
                    event: TapCreatureForMana,
                    condition: None,
                    intervening_if: false,
                    effect: AddMana {
                        who: Controller,
                        mana: ManaSpec {
                            produces: [
                                (
                                    Green,
                                    Fixed(
                                        1,
                                    ),
                                ),
                            ],
                            any_color: None,
                        },
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }
}
