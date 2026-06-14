//! Ba Sing Se — Land (first printed TLA, Avatar: The Last Airbender).
//!
//! Oracle:
//!   This land enters tapped unless you control a basic land.
//!   {T}: Add {G}.
//!   {2}{G}, {T}: Earthbend 2. Activate only as a sorcery.
//!
//! IMPLEMENTED:
//! - "enters tapped unless you control a basic land" — a `WouldEnterBattlefield(ItSelf)`
//!   replacement → `EntersTappedUnless(CountAtLeast{basic land ≥ 1})` (C11).
//! - `{T}: Add {G}` — a real IR mana ability (C19; it has no basic land type, so the mana is NOT
//!   intrinsic and needs the explicit ability).
//!
//! INCOMPLETE — TRACKED: "{2}{G}, {T}: Earthbend 2" needs the **earthbend** subsystem (C12 — a land
//! becomes a 0/0 haste creature that's still a land, gets +1/+1 counters, and on death/exile
//! returns tapped). The ability is omitted entirely rather than approximated. Flagged to engine.

use crate::basics::{CardType, Color, Zone};
use crate::cards::helpers::basic_land_filter;
use crate::cards::{mana_ability, CardDb, CardDef};
use crate::effects::ability::{Ability, ActionPattern, Rewrite};
use crate::effects::condition::Condition;
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::state::Characteristics;

/// grp id (per-set ids live near their cards).
pub const BA_SING_SE: u32 = 110;

pub fn register(db: &mut CardDb) {
    let chars = Characteristics {
        name: "Ba Sing Se".to_string(),
        card_types: vec![CardType::Land],
        grp_id: BA_SING_SE,
        ..Default::default()
    };
    db.insert(CardDef {
        chars,
        abilities: vec![
            // "{T}: Add {G}." (no basic land type → explicit IR mana ability, not intrinsic).
            mana_ability(Color::Green),
            // "enters tapped unless you control a basic land."
            Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersTappedUnless(Condition::CountAtLeast {
                    zone: Zone::Battlefield,
                    filter: basic_land_filter(),
                    controller: Some(PlayerRef::Controller),
                    n: ValueExpr::Fixed(1),
                }),
            },
        ],
        text: "This land enters tapped unless you control a basic land.\n{T}: Add {G}.\n{2}{G}, {T}: Earthbend 2. Activate only as a sorcery.".to_string(),
        fully_implemented: false,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn ba_sing_se_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BA_SING_SE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(def.is_mana_source()); // explicit {T}: Add {G} IR ability
        assert!(!def.fully_implemented); // earthbend ability deferred (C12)
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
                Replacement {
                    pattern: WouldEnterBattlefield(
                        ItSelf,
                    ),
                    rewrite: EntersTappedUnless(
                        CountAtLeast {
                            zone: Battlefield,
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
                            controller: Some(
                                Controller,
                            ),
                            n: Fixed(
                                1,
                            ),
                        },
                    ),
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
