//! Keen-Eyed Curator — `{G}{G}` Creature — Raccoon Scout 3/3 (first printed BLB, Bloomburrow).
//!
//! Oracle:
//!   As long as there are four or more card types among cards exiled with this creature, it gets
//!   +4/+4 and has trample.
//!   {1}: Exile target card from a graveyard.
//!
//! **Fully implemented** (no deferrals) — the first of the "hard" cards to land complete, on engine
//! cap C17 (e002d7a + b18c6f6):
//! - `{1}: Exile target card from a graveyard.` — an `Ability::Activated` ({1}, no other cost) over
//!   `Effect::Exile{ what: Target(CardInZone{ Graveyard, Any }) }`. The engine moves the targeted
//!   graveyard card to its owner's exile and records `Object.exiled_with = <this creature>` (the
//!   exile-association, CR 406/610), so the card is "exiled **with** this creature".
//! - "As long as there are four or more card types among cards exiled with this creature, it gets
//!   +4/+4 and has trample." — two `Ability::ConditionalStatic` on `ItSelf`, each gated on
//!   `Condition::ValueAtLeast(ValueExpr::DistinctCardTypesAmongExiledWith, Fixed(4))` (counts distinct
//!   card types among the objects whose `exiled_with` is this creature; evaluated source-aware). One
//!   contributes `ModifyPT{+4,+4}` (layer 7c), the other `GrantKeyword(Trample)` (layer 6) — both
//!   blink on/off exactly as the 4th distinct exiled card type appears/leaves.

use crate::basics::{Color, Zone};
use crate::cards::helpers::itself;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, Keyword, StaticContribution, Timing};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const KEEN_EYED_CURATOR: u32 = 117;

/// The shared condition for the buff: "four or more card types among cards exiled with this creature."
fn four_plus_exiled_types() -> Condition {
    Condition::ValueAtLeast(ValueExpr::DistinctCardTypesAmongExiledWith, ValueExpr::Fixed(4))
}

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            KEEN_EYED_CURATOR,
            "Keen-Eyed Curator",
            &[CreatureType::Raccoon, CreatureType::Scout],
            Color::Green,
            mana_cost(0, &[(Color::Green, 2)]),
            3,
            3,
            vec![
                // "{1}: Exile target card from a graveyard."
                Ability::Activated {
                    cost: Cost { mana: Some(mana_cost(1, &[])), components: vec![] },
                    effect: Effect::Exile {
                        what: EffectTarget::Target(TargetSpec {
                            kind: TargetKind::CardInZone {
                                zone: Zone::Graveyard,
                                filter: CardFilter::Any,
                            },
                            min: 1,
                            max: 1,
                            distinct: true,
                        }),
                    },
                    timing: Timing::Instant,
                    restriction: None,
                    is_mana: false,
                },
                // "… it gets +4/+4 …" while ≥4 card types among cards exiled with it.
                Ability::ConditionalStatic {
                    contribution: StaticContribution::ModifyPT { power: 4, toughness: 4 },
                    affects: itself(),
                    duration: Duration::WhileSourcePresent,
                    condition: four_plus_exiled_types(),
                },
                // "… and has trample." — same condition.
                Ability::ConditionalStatic {
                    contribution: StaticContribution::GrantKeyword(Keyword::Trample),
                    affects: itself(),
                    duration: Duration::WhileSourcePresent,
                    condition: four_plus_exiled_types(),
                },
            ],
        )
        .with_text("As long as there are four or more card types among cards exiled with this creature, it gets +4/+4 and has trample.\n{1}: Exile target card from a graveyard."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;
    use crate::subtypes::Subtype;
    use expect_test::expect;

    #[test]
    fn keen_eyed_curator_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(KEEN_EYED_CURATOR).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(
            def.chars.subtypes,
            vec![Subtype::Creature(CreatureType::Raccoon), Subtype::Creature(CreatureType::Scout)]
        );
        assert_eq!((def.chars.power, def.chars.toughness), (Some(3), Some(3)));
        assert!(def.fully_implemented); // both clauses faithful (C17 complete)
        expect![[r#"
            [
                Activated {
                    cost: Cost {
                        mana: Some(
                            ManaCost {
                                generic: 1,
                                colored: {},
                                x: 0,
                            },
                        ),
                        components: [],
                    },
                    effect: Exile {
                        what: Target(
                            TargetSpec {
                                kind: CardInZone {
                                    zone: Graveyard,
                                    filter: Any,
                                },
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                    },
                    timing: Instant,
                    restriction: None,
                    is_mana: false,
                },
                ConditionalStatic {
                    contribution: ModifyPT {
                        power: 4,
                        toughness: 4,
                    },
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: ItSelf,
                        chooser: Controller,
                        min: Fixed(
                            0,
                        ),
                        max: Fixed(
                            0,
                        ),
                    },
                    duration: WhileSourcePresent,
                    condition: ValueAtLeast(
                        DistinctCardTypesAmongExiledWith,
                        Fixed(
                            4,
                        ),
                    ),
                },
                ConditionalStatic {
                    contribution: GrantKeyword(
                        Trample,
                    ),
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: ItSelf,
                        chooser: Controller,
                        min: Fixed(
                            0,
                        ),
                        max: Fixed(
                            0,
                        ),
                    },
                    duration: WhileSourcePresent,
                    condition: ValueAtLeast(
                        DistinctCardTypesAmongExiledWith,
                        Fixed(
                            4,
                        ),
                    ),
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }
}
