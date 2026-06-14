//! Earthbender Ascension — `{2}{G}` Enchantment (first printed TLA, Avatar: The Last Airbender).
//!
//! Oracle:
//!   When this enchantment enters, earthbend 2. Then search your library for a basic land card, put
//!   it onto the battlefield tapped, then shuffle.
//!   Landfall — Whenever a land you control enters, put a quest counter on this enchantment. When you
//!   do, if it has four or more quest counters on it, put a +1/+1 counter on target creature you
//!   control. It gains trample until end of turn.
//!
//! IMPLEMENTED:
//! - "When this enchantment enters, **earthbend 2**. Then search your library for a basic land card,
//!   put it onto the battlefield tapped, then shuffle." — a `Triggered{SelfEnters}` over
//!   `Sequence[ Earthbend{target: land you control, n: 2}, fetch_basic_tapped() ]` (C12 + C5).
//!
//! INCOMPLETE — TRACKED (`fully_implemented: false`), two gaps, neither approximated:
//!   1. The **Landfall → quest-counter → reflexive "when you do" → intervening-if(≥4) → +1/+1 on
//!      target creature + trample-until-EOT** chain. Needs several unbuilt pieces: a `quest` counter
//!      kind, a *reflexive* ("when you do") trigger, an intervening-if on `CountersOnSelf ≥ 4`, and a
//!      grant-keyword-until-end-of-turn (trample). Omitted entirely until those caps land. Flagged.
//!   2. Earthbend's companion **"when it dies or is exiled, return it tapped"** delayed trigger is
//!      pending engine's earthbend **commit C** (engine-internal; no card change when it lands).

use crate::basics::Color;
use crate::cards::helpers::{earthbend, fetch_basic_tapped};
use crate::cards::{enchantment, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const EARTHBENDER_ASCENSION: u32 = 114;

pub fn register(db: &mut CardDb) {
    let mut def = enchantment(
        EARTHBENDER_ASCENSION,
        "Earthbender Ascension",
        Color::Green,
        mana_cost(2, &[(Color::Green, 1)]),
        vec![
            // "When this enchantment enters, earthbend 2. Then search your library for a basic land
            // card, put it onto the battlefield tapped, then shuffle."
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::Sequence(vec![earthbend(2), fetch_basic_tapped()]),
            },
        ],
    );
    def.text = "When this enchantment enters, earthbend 2. Then search your library for a basic land card, put it onto the battlefield tapped, then shuffle.\nLandfall — Whenever a land you control enters, put a quest counter on this enchantment. When you do, if it has four or more quest counters on it, put a +1/+1 counter on target creature you control. It gains trample until end of turn.".to_string();
    // Tracked-incomplete: the landfall/quest-counter/reflexive chain is unbuilt; earthbend's
    // return-tapped clause is pending engine commit C. See module docs.
    def.fully_implemented = false;
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;
    use expect_test::expect;

    #[test]
    fn earthbender_ascension_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(EARTHBENDER_ASCENSION).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Enchantment]);
        // Tracked-incomplete: landfall/quest chain unbuilt + earthbend return-tapped pending C.
        assert!(!def.fully_implemented);
        // Only the ETB earthbend-then-fetch is materialized; the landfall chain is deliberately
        // absent (no silent approximation).
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Sequence(
                        [
                            Earthbend {
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
                                    2,
                                ),
                            },
                            Search {
                                who: Controller,
                                zone: Library,
                                filter: All(
                                    [
                                        HasCardType(
                                            Land,
                                        ),
                                        Supertype(
                                            Basic,
                                        ),
                                    ],
                                ),
                                min: 0,
                                max: 1,
                                to: ZoneDest {
                                    zone: Battlefield,
                                    pos: Any,
                                },
                                tapped: true,
                            },
                        ],
                    ),
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }
}
