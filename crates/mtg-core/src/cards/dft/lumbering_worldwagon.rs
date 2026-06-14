//! Lumbering Worldwagon — `{2}{G}` Artifact — Vehicle `*`/4 (first printed DFT, Aetherdrift).
//!
//! Oracle:
//!   This Vehicle's power is equal to the number of lands you control.
//!   Whenever this Vehicle enters or attacks, you may search your library for a basic land card,
//!   put it onto the battlefield tapped, then shuffle.
//!   Crew 4
//!
//! IMPLEMENTED:
//! - `*`/4 characteristic-defining ability (CR 604.3 / 613.4b layer 7a) via
//!   `StaticContribution::SetBasePTValue { power = Count(lands you control), toughness = 4 }`
//!   (C9b). Base printed power is 0; the CDA sets it.
//! - "enters or attacks → may fetch a basic to the battlefield tapped" as two triggered
//!   abilities (SelfEnters, SelfAttacks), each an `Optional` over a `Search` (C5).
//!
//! INCOMPLETE — TRACKED (needs the unbuilt **Crew** subsystem, CR 702.122):
//!   • "Crew 4" — tap creatures with total power ≥ 4 to turn this into an artifact creature until
//!     end of turn. Until Crew exists this Vehicle never becomes a creature, so its CDA power is
//!     moot and the *attacks* trigger can't fire — but the IR above is faithful, not approximated.
//!   Flagged to engine/lead.

use crate::basics::{CardType, Color};
use crate::cards::helpers::{fetch_basic_tapped, itself, lands_you_control};
use crate::cards::{mana_cost, CardDb, CardDef};
use crate::effects::ability::{Ability, EventPattern, StaticContribution};
use crate::effects::condition::Duration;
use crate::effects::value::ValueExpr;
use crate::effects::Effect;
use crate::state::Characteristics;
use crate::subtypes::ArtifactType;

/// grp id (per-set ids live near their cards).
pub const LUMBERING_WORLDWAGON: u32 = 105;

/// "you may search your library for a basic land card, put it onto the battlefield tapped, then
/// shuffle" — the shared body of both triggers.
fn may_fetch_basic_tapped() -> Effect {
    Effect::Optional {
        prompt: "Search your library for a basic land card to put onto the battlefield tapped?".to_string(),
        body: Box::new(fetch_basic_tapped()),
    }
}

pub fn register(db: &mut CardDb) {
    let def = CardDef {
        chars: Characteristics {
            name: "Lumbering Worldwagon".to_string(),
            card_types: vec![CardType::Artifact],
            subtypes: vec![ArtifactType::Vehicle.into()],
            colors: vec![Color::Green],
            mana_cost: Some(mana_cost(2, &[(Color::Green, 1)])),
            // `*` printed power → base 0; the layer-7a CDA sets the real value.
            power: Some(0),
            toughness: Some(4),
            grp_id: LUMBERING_WORLDWAGON,
            ..Default::default()
        },
        abilities: vec![
            // `*`/4 CDA (layer 7a).
            Ability::Static {
                contribution: StaticContribution::SetBasePTValue {
                    power: lands_you_control(),
                    toughness: ValueExpr::Fixed(4),
                },
                affects: itself(),
                duration: Duration::WhileSourcePresent,
            },
            // "Whenever this Vehicle enters … you may fetch a basic."
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: may_fetch_basic_tapped(),
            },
            // "… or attacks, you may fetch a basic." (Can't fire until Crew exists — see docs.)
            Ability::Triggered {
                event: EventPattern::SelfAttacks,
                condition: None,
                intervening_if: false,
                effect: may_fetch_basic_tapped(),
            },
        ],
        text: "This Vehicle's power is equal to the number of lands you control.\nWhenever this Vehicle enters or attacks, you may search your library for a basic land card, put it onto the battlefield tapped, then shuffle.\nCrew 4".to_string(),
        // Tracked-incomplete: Crew 4 is an unbuilt subsystem (see module docs).
        fully_implemented: false,
    };
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn lumbering_worldwagon_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(LUMBERING_WORLDWAGON).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Artifact]);
        assert_eq!(def.chars.subtypes, vec![ArtifactType::Vehicle.into()]);
        assert_eq!(def.chars.toughness, Some(4));
        assert!(!def.is_mana_source());
        expect![[r#"
            [
                Static {
                    contribution: SetBasePTValue {
                        power: Count {
                            zone: Battlefield,
                            filter: All(
                                [
                                    HasCardType(
                                        Land,
                                    ),
                                    ControlledBy(
                                        Controller,
                                    ),
                                ],
                            ),
                            controller: Some(
                                Controller,
                            ),
                        },
                        toughness: Fixed(
                            4,
                        ),
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
                },
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Optional {
                        prompt: "Search your library for a basic land card to put onto the battlefield tapped?",
                        body: Search {
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
                    },
                },
                Triggered {
                    event: SelfAttacks,
                    condition: None,
                    intervening_if: false,
                    effect: Optional {
                        prompt: "Search your library for a basic land card to put onto the battlefield tapped?",
                        body: Search {
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
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
