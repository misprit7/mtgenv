//! Badgermole Cub — `{1}{G}` Creature — Badger Mole 2/2 (first printed TLA, Avatar: The Last
//! Airbender).
//!
//! Oracle:
//!   When this creature enters, earthbend 1. (Target land you control becomes a 0/0 creature with
//!   haste that's still a land. Put a +1/+1 counter on it. When it dies or is exiled, return it to
//!   the battlefield tapped.)
//!   Whenever you tap a creature for mana, add an additional {G}.
//!
//! IMPLEMENTED:
//! - "When this creature enters, **earthbend 1**" — a `Triggered{SelfEnters}` over
//!   `Effect::Earthbend{target: target land you control, n: 1}` (C12). The targeted land becomes a
//!   0/0 haste land-creature with one +1/+1 counter.
//!
//! INCOMPLETE — TRACKED (`fully_implemented: false`), two distinct gaps, neither approximated:
//!   1. **"Whenever you tap a creature for mana, add an additional {G}."** A *reflexive mana
//!      trigger* (CR 605 / a trigger on the "tap a creature for mana" event that itself adds mana) —
//!      an unbuilt subsystem (no `EventPattern` for "tapped a creature for mana", and mana-adding
//!      triggers are a special no-stack case). Omitted entirely until that cap lands. Flagged to engine.
//!   2. Earthbend's companion **"when it dies or is exiled, return it tapped"** delayed trigger is
//!      pending engine's earthbend **commit C** (an engine-internal materialization step; no card
//!      change when it lands).

use crate::basics::Color;
use crate::cards::helpers::earthbend;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
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
        ],
    );
    def.text = "When this creature enters, earthbend 1. (Target land you control becomes a 0/0 creature with haste that's still a land. Put a +1/+1 counter on it. When it dies or is exiled, return it to the battlefield tapped.)\nWhenever you tap a creature for mana, add an additional {G}.".to_string();
    // Tracked-incomplete: the reflexive "tap a creature for mana → add {G}" trigger is an unbuilt
    // subsystem; earthbend's return-tapped clause is pending engine commit C. See module docs.
    def.fully_implemented = false;
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
        // Tracked-incomplete: reflexive mana trigger unbuilt + earthbend return-tapped pending C.
        assert!(!def.fully_implemented);
        // Only the ETB earthbend trigger is materialized; the reflexive mana trigger is deliberately
        // absent (no silent approximation). Earthbend targets "a land you control".
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
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }
}
