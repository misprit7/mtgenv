//! Dyadrine, Synthesis Amalgam — `{X}{G}{W}` Legendary Artifact Creature — Construct 0/1 (first
//! printed EOE, Edge of Eternities).
//!
//! Oracle:
//!   Trample
//!   Dyadrine enters with a number of +1/+1 counters on it equal to the amount of mana spent to cast
//!   it.
//!   Whenever you attack, you may remove a +1/+1 counter from each of two creatures you control. If
//!   you do, draw a card and create a 2/2 colorless Robot artifact creature token.
//!
//! **Fully implemented:**
//! - **Trample** (CR 702.19) — printed `Keyword`.
//! - **"Enters with +1/+1 counters equal to the mana spent to cast it"** — a `WouldEnterBattlefield(
//!   ItSelf)` replacement → `Rewrite::EntersWithCountersValue { PlusOnePlusOne, n: ValueExpr::ManaSpent }`
//!   (engine cap a2e2b13). `ManaSpent` = total mana paid at cast (generic + colored + the chosen X),
//!   reset on any zone change (CR 400.7), so Dyadrine cast for {3}{G}{W} (X=3) enters as a 5/6.
//! - **"Whenever you attack, you may remove a +1/+1 counter from each of two creatures you control. If
//!   you do, draw a card and create a 2/2 colorless Robot artifact creature token."** — a
//!   `Triggered{YouAttack}` over `Optional{ Sequence[ ForEach{ select 2 creatures you control with a
//!   +1/+1 counter, body: PutCounters{Each, -1} }, Draw, CreateToken{Robot} ] }` (cap 0e01d56). It's a
//!   *resolution-time choice* (no "target") — the `Optional` + `Select(min:2)` gate the reward, so the
//!   draw + token fire only when you actually remove a counter from each of two creatures (CR 603.7
//!   reflexive semantics without a separate sub-trigger).

use crate::basics::{CardType, Color, CounterKind, Zone};
use crate::cards::{mana_cost, CardDb, CardDef};
use crate::effects::ability::{Ability, ActionPattern, EventPattern, Keyword, Rewrite};
use crate::effects::target::{CardFilter, SelectSpec, TokenSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::state::Characteristics;
use crate::subtypes::{CreatureType, Subtype, Supertype};

/// grp id (per-set ids live near their cards).
pub const DYADRINE_SYNTHESIS_AMALGAM: u32 = 116;

pub fn register(db: &mut CardDb) {
    // {X}{G}{W}: generic 0, one G + one W pip, one {X} symbol.
    let mut cost = mana_cost(0, &[(Color::Green, 1), (Color::White, 1)]);
    cost.x = 1;
    let chars = Characteristics {
        name: "Dyadrine, Synthesis Amalgam".to_string(),
        card_types: vec![CardType::Artifact, CardType::Creature],
        subtypes: vec![CreatureType::Construct.into()],
        supertypes: vec![Supertype::Legendary],
        colors: vec![Color::Green, Color::White],
        mana_cost: Some(cost),
        power: Some(0),
        toughness: Some(1),
        keywords: vec![Keyword::Trample],
        grp_id: DYADRINE_SYNTHESIS_AMALGAM,
        ..Default::default()
    };
    db.insert(CardDef {
        chars,
        abilities: vec![
            // "Dyadrine enters with a number of +1/+1 counters on it equal to the mana spent to cast it."
            Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersWithCountersValue {
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::ManaSpent,
                },
            },
            // "Whenever you attack, you may remove a +1/+1 counter from each of two creatures you
            // control. If you do, draw a card and create a 2/2 colorless Robot artifact creature token."
            Ability::Triggered {
                event: EventPattern::YouAttack,
                condition: None,
                intervening_if: false,
                effect: Effect::Optional {
                    prompt: "Remove a +1/+1 counter from each of two creatures you control?".to_string(),
                    body: Box::new(Effect::Sequence(vec![
                        // remove a +1/+1 counter from each of two chosen creatures you control with one
                        Effect::ForEach {
                            selector: SelectSpec {
                                zone: Zone::Battlefield,
                                filter: CardFilter::All(vec![
                                    CardFilter::HasCardType(CardType::Creature),
                                    CardFilter::ControlledBy(PlayerRef::Controller),
                                    CardFilter::HasCounter(CounterKind::PlusOnePlusOne),
                                ]),
                                chooser: PlayerRef::Controller,
                                min: ValueExpr::Fixed(2),
                                max: ValueExpr::Fixed(2),
                            },
                            body: Box::new(Effect::PutCounters {
                                what: EffectTarget::Each,
                                kind: CounterKind::PlusOnePlusOne,
                                n: ValueExpr::Fixed(-1),
                            }),
                        },
                        // "If you do, draw a card and create a 2/2 colorless Robot artifact creature token."
                        Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
                        Effect::CreateToken {
                            spec: TokenSpec {
                                name: "Robot".to_string(),
                                card_types: vec![CardType::Artifact, CardType::Creature],
                                subtypes: vec![Subtype::Creature(CreatureType::Robot)],
                                colors: vec![],
                                power: 2,
                                toughness: 2,
                                keywords: vec![],
                                counters: vec![],
                            },
                            count: ValueExpr::Fixed(1),
                            controller: PlayerRef::Controller,
                        },
                    ])),
                },
            },
        ],
        text: "Trample\nDyadrine enters with a number of +1/+1 counters on it equal to the amount of mana spent to cast it.\nWhenever you attack, you may remove a +1/+1 counter from each of two creatures you control. If you do, draw a card and create a 2/2 colorless Robot artifact creature token.".to_string(),
        // Fully implemented: trample + enters-with-counters (ManaSpent) + the attack ability (cap 0e01d56).
        fully_implemented: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subtypes::Subtype;
    use expect_test::expect;

    #[test]
    fn dyadrine_synthesis_amalgam_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(DYADRINE_SYNTHESIS_AMALGAM).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Artifact, CardType::Creature]);
        assert_eq!(def.chars.subtypes, vec![Subtype::Creature(CreatureType::Construct)]);
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!(def.chars.colors, vec![Color::Green, Color::White]);
        assert_eq!(def.chars.keywords, vec![Keyword::Trample]); // trample works today
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().x, 1); // {X} symbol present
        assert_eq!((def.chars.power, def.chars.toughness), (Some(0), Some(1))); // base; counters add
        // Fully implemented: trample + enters-with-counters(ManaSpent) + the YouAttack ability.
        assert!(def.fully_implemented);
        // enters-with-counters-=-mana-spent replacement + the "whenever you attack" Optional/ForEach ability.
        expect![[r#"
            [
                Replacement {
                    pattern: WouldEnterBattlefield(
                        ItSelf,
                    ),
                    rewrite: EntersWithCountersValue {
                        kind: PlusOnePlusOne,
                        n: ManaSpent,
                    },
                },
                Triggered {
                    event: YouAttack,
                    condition: None,
                    intervening_if: false,
                    effect: Optional {
                        prompt: "Remove a +1/+1 counter from each of two creatures you control?",
                        body: Sequence(
                            [
                                ForEach {
                                    selector: SelectSpec {
                                        zone: Battlefield,
                                        filter: All(
                                            [
                                                HasCardType(
                                                    Creature,
                                                ),
                                                ControlledBy(
                                                    Controller,
                                                ),
                                                HasCounter(
                                                    PlusOnePlusOne,
                                                ),
                                            ],
                                        ),
                                        chooser: Controller,
                                        min: Fixed(
                                            2,
                                        ),
                                        max: Fixed(
                                            2,
                                        ),
                                    },
                                    body: PutCounters {
                                        what: Each,
                                        kind: PlusOnePlusOne,
                                        n: Fixed(
                                            -1,
                                        ),
                                    },
                                },
                                Draw {
                                    who: Controller,
                                    count: Fixed(
                                        1,
                                    ),
                                },
                                CreateToken {
                                    spec: TokenSpec {
                                        name: "Robot",
                                        card_types: [
                                            Artifact,
                                            Creature,
                                        ],
                                        subtypes: [
                                            Creature(
                                                Robot,
                                            ),
                                        ],
                                        colors: [],
                                        power: 2,
                                        toughness: 2,
                                        keywords: [],
                                        counters: [],
                                    },
                                    count: Fixed(
                                        1,
                                    ),
                                    controller: Controller,
                                },
                            ],
                        ),
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }
}
