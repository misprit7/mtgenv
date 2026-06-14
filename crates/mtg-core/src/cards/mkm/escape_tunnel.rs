//! Escape Tunnel — Land (first printed MKM, Murders at Karlov Manor).
//!
//! Oracle:
//!   {T}, Sacrifice this land: Search your library for a basic land card, put it onto the
//!   battlefield tapped, then shuffle.
//!   {T}, Sacrifice this land: Target creature with power 2 or less can't be blocked this turn.
//!
//! IMPLEMENTED: the first ability — the fetch (`{T}, Sacrifice this:` → a basic onto the
//! battlefield tapped, C5), an instant-speed activated ability.
//!
//! INCOMPLETE — TRACKED: the second ability ("target creature with power 2 or less can't be
//! blocked this turn") needs a `CantBeBlocked` qualification (unbuilt). The ability is omitted
//! entirely rather than approximated. Flagged to engine.

use crate::basics::CardType;
use crate::cards::helpers::{fetch_basic_tapped, sacrifice_self};
use crate::cards::{CardDb, CardDef};
use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
use crate::state::Characteristics;

/// grp id (per-set ids live near their cards).
pub const ESCAPE_TUNNEL: u32 = 107;

pub fn register(db: &mut CardDb) {
    let chars = Characteristics {
        name: "Escape Tunnel".to_string(),
        card_types: vec![CardType::Land],
        grp_id: ESCAPE_TUNNEL,
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
        text: "{T}, Sacrifice this land: Search your library for a basic land card, put it onto the battlefield tapped, then shuffle.\n{T}, Sacrifice this land: Target creature with power 2 or less can't be blocked this turn.".to_string(),
        fully_implemented: false,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn escape_tunnel_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ESCAPE_TUNNEL).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(!def.is_mana_source());
        assert!(!def.fully_implemented); // the unblockable ability is deferred (see module docs)
        // Only the fetch ability is present (the "can't be blocked" ability is TRACKED-incomplete).
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
