//! Earthbender Ascension — `{2}{G}` Enchantment (first printed TLA, Avatar: The Last Airbender).
//!
//! Oracle:
//!   When this enchantment enters, earthbend 2. Then search your library for a basic land card, put
//!   it onto the battlefield tapped, then shuffle.
//!   Landfall — Whenever a land you control enters, put a quest counter on this enchantment. When you
//!   do, if it has four or more quest counters on it, put a +1/+1 counter on target creature you
//!   control. It gains trample until end of turn.
//!
//! **Fully implemented** — both abilities faithful:
//! - "When this enchantment enters, **earthbend 2**. Then search your library for a basic land card,
//!   put it onto the battlefield tapped, then shuffle." — a `Triggered{SelfEnters}` over
//!   `Sequence[ Earthbend{target: land you control, n: 2}, fetch_basic_tapped() ]` (C12 + C5).
//!   (Earthbend, incl. its "dies/exiled → return tapped" delayed trigger, fully landed in C12.)
//! - "Landfall — Whenever a land you control enters, put a quest counter on this enchantment. When you
//!   do, if it has four or more quest counters on it, put a +1/+1 counter on target creature you
//!   control. It gains trample until end of turn." — a `Triggered{PermanentEnters(land you control)}`
//!   over `Sequence[ PutCounters{SourceSelf, Named("quest"), 1}, Conditional{ ValueAtLeast(
//!   CountersOnSelf(Named("quest")), 4), then: [ +1/+1 on target creature you control, trample until
//!   EOT ] } ]`. The "When you do … if ≥4 … target creature" is a **reflexive sub-trigger** (CR 603.7c,
//!   cap 2e13694): the quest counter is put unconditionally, then *only if* ≥4 quest counters does the
//!   reflexive ability go on the stack and choose its target (so sub-4 landfalls never prompt a target,
//!   and the counter always lands even with no creatures). `GrantKeyword{ChosenIndex(0), Trample,
//!   UntilEndOfTurn}` reuses the +1/+1's chosen creature.

use crate::basics::{Color, CounterKind};
use crate::cards::helpers::{earthbend, fetch_basic_tapped, land_you_control};
use crate::cards::{enchantment, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

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
            // "Landfall — Whenever a land you control enters, put a quest counter on this enchantment.
            // When you do, if it has four or more quest counters on it, put a +1/+1 counter on target
            // creature you control. It gains trample until end of turn."
            Ability::Triggered {
                event: EventPattern::PermanentEnters(land_you_control()),
                condition: None,
                intervening_if: false,
                effect: Effect::Sequence(vec![
                    Effect::PutCounters {
                        what: EffectTarget::SourceSelf,
                        kind: CounterKind::Named("quest".to_string()),
                        n: ValueExpr::Fixed(1),
                    },
                    // "When you do, if ≥4 quest counters …" — a reflexive sub-trigger (CR 603.7c): the
                    // targeted reward is deferred, its target chosen only when the intervening-if holds.
                    Effect::Conditional {
                        cond: Condition::ValueAtLeast(
                            ValueExpr::CountersOnSelf(CounterKind::Named("quest".to_string())),
                            ValueExpr::Fixed(4),
                        ),
                        then: Box::new(Effect::Sequence(vec![
                            Effect::PutCounters {
                                what: EffectTarget::Target(TargetSpec {
                                    kind: TargetKind::Creature(CardFilter::ControlledBy(
                                        PlayerRef::Controller,
                                    )),
                                    min: 1,
                                    max: 1,
                                    distinct: true,
                                }),
                                kind: CounterKind::PlusOnePlusOne,
                                n: ValueExpr::Fixed(1),
                            },
                            // "It gains trample until end of turn." — the same chosen creature.
                            Effect::GrantKeyword {
                                what: EffectTarget::ChosenIndex(0),
                                keyword: Keyword::Trample,
                                duration: Duration::UntilEndOfTurn,
                            },
                        ])),
                        otherwise: None,
                    },
                ]),
            },
        ],
    );
    def.text = "When this enchantment enters, earthbend 2. Then search your library for a basic land card, put it onto the battlefield tapped, then shuffle.\nLandfall — Whenever a land you control enters, put a quest counter on this enchantment. When you do, if it has four or more quest counters on it, put a +1/+1 counter on target creature you control. It gains trample until end of turn.".to_string();
    // Fully implemented: ETB earthbend+fetch (C12+C5) + the landfall quest-chain with the reflexive
    // sub-trigger reward (cap 2e13694). See module docs.
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
        // Fully implemented: ETB earthbend+fetch + the landfall quest-chain (reflexive reward).
        assert!(def.fully_implemented);
        // ETB earthbend-then-fetch + landfall → quest counter → reflexive Conditional(≥4) reward.
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
                Triggered {
                    event: PermanentEnters(
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
                    condition: None,
                    intervening_if: false,
                    effect: Sequence(
                        [
                            PutCounters {
                                what: SourceSelf,
                                kind: Named(
                                    "quest",
                                ),
                                n: Fixed(
                                    1,
                                ),
                            },
                            Conditional {
                                cond: ValueAtLeast(
                                    CountersOnSelf(
                                        Named(
                                            "quest",
                                        ),
                                    ),
                                    Fixed(
                                        4,
                                    ),
                                ),
                                then: Sequence(
                                    [
                                        PutCounters {
                                            what: Target(
                                                TargetSpec {
                                                    kind: Creature(
                                                        ControlledBy(
                                                            Controller,
                                                        ),
                                                    ),
                                                    min: 1,
                                                    max: 1,
                                                    distinct: true,
                                                },
                                            ),
                                            kind: PlusOnePlusOne,
                                            n: Fixed(
                                                1,
                                            ),
                                        },
                                        GrantKeyword {
                                            what: ChosenIndex(
                                                0,
                                            ),
                                            keyword: Trample,
                                            duration: UntilEndOfTurn,
                                        },
                                    ],
                                ),
                                otherwise: None,
                            },
                        ],
                    ),
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }
}
