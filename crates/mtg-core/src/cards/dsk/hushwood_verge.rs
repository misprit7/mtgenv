//! Hushwood Verge — Land (first printed DSK, Duskmourn). A Selesnya (G/W) "Verge" dual.
//!
//! Oracle:
//!   {T}: Add {G}.
//!   {T}: Add {W}. Activate only if you control a Forest or a Plains.
//!
//! Fully implemented (no approximation): two first-class IR mana abilities (C19). The {G} is
//! unconditional; the {W} carries `Restriction::OnlyIf(Condition::CountAtLeast{Forest/Plains ≥ 1})`
//! so the engine only offers it when you control a Forest or a Plains — faithful to the printed
//! activation restriction (this previously tapped unconditionally via `mana_colors`, which was wrong).

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_ability, CardDb, CardDef};
use crate::effects::ability::{Ability, Cost, CostComponent, Restriction, Timing};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, ManaSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::state::Characteristics;

pub const HUSHWOOD_VERGE: u32 = 101;

pub fn register(db: &mut CardDb) {
    let chars = Characteristics {
        name: "Hushwood Verge".to_string(),
        card_types: vec![CardType::Land],
        grp_id: HUSHWOOD_VERGE,
        ..Default::default()
    };
    db.insert(CardDef {
        chars,
        abilities: vec![
            // "{T}: Add {G}." — unconditional.
            mana_ability(Color::Green),
            // "{T}: Add {W}. Activate only if you control a Forest or a Plains."
            Ability::Activated {
                cost: Cost {
                    mana: None,
                    components: vec![CostComponent::TapSelf],
                },
                effect: Effect::AddMana {
                    who: PlayerRef::Controller,
                    mana: ManaSpec {
                        produces: vec![(Color::White, ValueExpr::Fixed(1))],
                        any_color: None,
                    },
                },
                timing: Timing::Instant,
                restriction: Some(Restriction::OnlyIf(Condition::CountAtLeast {
                    zone: Zone::Battlefield,
                    filter: CardFilter::AnyOf(vec![
                        CardFilter::HasSubtype("Forest".to_string()),
                        CardFilter::HasSubtype("Plains".to_string()),
                    ]),
                    controller: Some(PlayerRef::Controller),
                    n: ValueExpr::Fixed(1),
                })),
                is_mana: true,
            },
        ],
        mana_colors: Vec::new(),
        text: "{T}: Add {G}.\n{T}: Add {W}. Activate only if you control a Forest or a Plains."
            .to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn hushwood_verge_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(HUSHWOOD_VERGE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(def.chars.mana_cost.is_none()); // lands aren't cast
        assert!(def.mana_colors.is_empty()); // mana is first-class IR now, not the shortcut
        assert!(def.is_mana_source());
        // Two mana abilities: unconditional {G}, and {W} gated on controlling a Forest/Plains.
        expect![[r#"
            [
                Activated {
                    cost: Cost {
                        mana: None,
                        components: [
                            TapSelf,
                        ],
                    },
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
                    timing: Instant,
                    restriction: None,
                    is_mana: true,
                },
                Activated {
                    cost: Cost {
                        mana: None,
                        components: [
                            TapSelf,
                        ],
                    },
                    effect: AddMana {
                        who: Controller,
                        mana: ManaSpec {
                            produces: [
                                (
                                    White,
                                    Fixed(
                                        1,
                                    ),
                                ),
                            ],
                            any_color: None,
                        },
                    },
                    timing: Instant,
                    restriction: Some(
                        OnlyIf(
                            CountAtLeast {
                                zone: Battlefield,
                                filter: AnyOf(
                                    [
                                        HasSubtype(
                                            "Forest",
                                        ),
                                        HasSubtype(
                                            "Plains",
                                        ),
                                    ],
                                ),
                                controller: Some(
                                    Controller,
                                ),
                                n: Fixed(
                                    1,
                                ),
                            },
                        ),
                    ),
                    is_mana: true,
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
