//! Deathcap Glade — Land (first printed VOW, Innistrad: Crimson Vow; reprinted in SOS).
//!
//! Oracle: "This land enters tapped unless you control two or more other lands. / {T}: Add {B} or
//! {G}."
//!
//! **Fully implemented** — the shared `checkland` builder: an `EntersTappedUnless(CountAtLeast{lands
//! ≥ 2})` replacement (counting your other lands as it enters) plus two `{T}: Add` mana abilities
//! (B, G).

use crate::basics::Color;
use crate::cards::{checkland, CardDb};

/// grp id (per-set ids live near their cards).
pub const DEATHCAP_GLADE: u32 = 243;

pub fn register(db: &mut CardDb) {
    db.insert(
        checkland(DEATHCAP_GLADE, "Deathcap Glade", Color::Black, Color::Green)
            .with_text("This land enters tapped unless you control two or more other lands.\n{T}: Add {B} or {G}."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;
    use expect_test::expect;

    #[test]
    fn deathcap_glade_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(DEATHCAP_GLADE).unwrap();
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
                            one_of: None,
                            restriction: None,
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
                            one_of: None,
                            restriction: None,
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
                    rewrite: EntersTappedUnless(
                        CountAtLeast {
                            zone: Battlefield,
                            filter: HasCardType(
                                Land,
                            ),
                            controller: Some(
                                Controller,
                            ),
                            n: Fixed(
                                2,
                            ),
                        },
                    ),
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
