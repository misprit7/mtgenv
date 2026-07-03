//! Titan's Grave — Land (first printed SOS).
//!
//! Oracle: "This land enters tapped. / {T}: Add {B} or {G}. / {2}{B}{G}, {T}: Surveil 1."
//!
//! **Fully implemented** — the shared `surveil_dual` builder: enters tapped, two `{T}: Add` mana
//! abilities (B, G), and a `{2}{B}{G}, {T}: Surveil 1` activated ability.

use crate::basics::Color;
use crate::cards::{surveil_dual, CardDb};

/// grp id (per-set ids live near their cards).
pub const TITANS_GRAVE: u32 = 248;

pub fn register(db: &mut CardDb) {
    db.insert(
        surveil_dual(TITANS_GRAVE, "Titan's Grave", Color::Black, Color::Green)
            .with_text("This land enters tapped.\n{T}: Add {B} or {G}.\n{2}{B}{G}, {T}: Surveil 1."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;
    use expect_test::expect;

    #[test]
    fn titans_grave_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TITANS_GRAVE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(def.is_mana_source());
        assert!(def.fully_implemented);
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
                                    Black,
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
                Replacement {
                    pattern: WouldEnterBattlefield(
                        ItSelf,
                    ),
                    rewrite: EntersTapped,
                },
                Activated {
                    cost: Cost {
                        mana: Some(
                            ManaCost {
                                generic: 2,
                                colored: {
                                    Black: 1,
                                    Green: 1,
                                },
                                x: 0,
                                hybrid: [],
                                mono_hybrid: [],
                            },
                        ),
                        components: [
                            TapSelf,
                        ],
                    },
                    effect: Surveil {
                        count: Fixed(
                            1,
                        ),
                    },
                    timing: Instant,
                    restriction: None,
                    is_mana: false,
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
