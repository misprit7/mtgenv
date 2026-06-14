//! Fabled Passage — Land (first printed ELD, Throne of Eldraine).
//!
//! Oracle: "{T}, Sacrifice this land: Search your library for a basic land card, put it onto the
//! battlefield tapped, then shuffle. Then if you control four or more lands, untap that land."
//!
//! IMPLEMENTED: the fetch — `{T}, Sacrifice this:` (TapSelf + Sacrifice `ItSelf`) → search a basic
//! onto the battlefield tapped (C5), as an instant-speed activated ability.
//!
//! INCOMPLETE — TRACKED: "Then if you control four or more lands, untap that land." Needs a handle
//! to reference *the just-searched permanent* (an unbuilt capability) + a `CountAtLeast` condition.
//! Until then the fetched land simply stays tapped — a faithful subset (a missing upside), NOT a
//! wrong approximation. Flagged to engine.

use crate::basics::CardType;
use crate::cards::helpers::{fetch_basic_tapped, sacrifice_self};
use crate::cards::{CardDb, CardDef};
use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
use crate::state::Characteristics;

/// grp id (per-set ids live near their cards).
pub const FABLED_PASSAGE: u32 = 106;

pub fn register(db: &mut CardDb) {
    let chars = Characteristics {
        name: "Fabled Passage".to_string(),
        card_types: vec![CardType::Land],
        grp_id: FABLED_PASSAGE,
        ..Default::default()
    };
    db.insert(CardDef {
        chars,
        abilities: vec![Ability::Activated {
            cost: Cost {
                mana: None,
                components: vec![
                    CostComponent::TapSelf,
                    CostComponent::Sacrifice(sacrifice_self()),
                ],
            },
            effect: fetch_basic_tapped(),
            timing: Timing::Instant,
            restriction: None,
            is_mana: false,
        }],
        text: "{T}, Sacrifice this land: Search your library for a basic land card, put it onto the battlefield tapped, then shuffle. Then if you control four or more lands, untap that land.".to_string(),
        fully_implemented: false,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn fabled_passage_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(FABLED_PASSAGE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(!def.is_mana_source()); // a fetch, not a mana source
        assert!(!def.fully_implemented); // "untap that land" deferred (see module docs)
        expect![[r#"
            [
                Activated {
                    cost: Cost {
                        mana: None,
                        components: [
                            TapSelf,
                            Sacrifice(
                                SelectSpec {
                                    zone: Battlefield,
                                    filter: ItSelf,
                                    chooser: Controller,
                                    min: Fixed(
                                        1,
                                    ),
                                    max: Fixed(
                                        1,
                                    ),
                                },
                            ),
                        ],
                    },
                    effect: Search {
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
                    timing: Instant,
                    restriction: None,
                    is_mana: false,
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
