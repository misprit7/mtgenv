//! Lumbering Worldwagon — `{2}{G}` Artifact — Vehicle `*`/4 (first printed DFT, Aetherdrift).
//!
//! Oracle:
//!   This Vehicle's power is equal to the number of lands you control.
//!   Whenever this Vehicle enters or attacks, you may search your library for a basic land card,
//!   put it onto the battlefield tapped, then shuffle.
//!   Crew 4
//!
//! **Fully implemented:**
//! - `*`/4 characteristic-defining ability (CR 604.3 / 613.4b layer 7a) via
//!   `StaticContribution::SetBasePTValue { power = Count(lands you control), toughness = 4 }`
//!   (C9b). Base printed power is 0; the CDA sets it.
//! - "enters or attacks → may fetch a basic to the battlefield tapped" as two triggered
//!   abilities (SelfEnters, SelfAttacks), each an `Optional` over a `Search` (C5).
//! - **Crew 4** (CR 702.122, cap `80d9ab3`) — an `Activated{ cost: Crew(4), BecomeCreature{ SourceSelf,
//!   UntilEndOfTurn } }`: tap untapped creatures with total power ≥ 4 → the Vehicle gains the creature
//!   type until end of turn (`GrantContinuous{AddType(Creature)}`, keeping its `*`/4 CDA + artifact
//!   type). Once crewed it can attack, so its `*` power and the *attacks*-trigger fetch both come live.

use crate::basics::{CardType, Color};
use crate::cards::helpers::{fetch_basic_tapped, itself, lands_you_control};
use crate::cards::{mana_cost, CardDb, CardDef};
use crate::effects::ability::{Ability, Cost, CostComponent, EventPattern, StaticContribution, Timing};
use crate::effects::condition::Duration;
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
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
            // "… or attacks, you may fetch a basic." (Now live — Crew can animate it to attack.)
            Ability::Triggered {
                event: EventPattern::SelfAttacks,
                condition: None,
                intervening_if: false,
                effect: may_fetch_basic_tapped(),
            },
            // "Crew 4" — tap untapped creatures with total power ≥ 4 → becomes an artifact creature
            // until end of turn (it keeps its */4 CDA + Vehicle/artifact types).
            Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::Crew(4)] },
                effect: Effect::BecomeCreature {
                    what: EffectTarget::SourceSelf,
                    duration: Duration::UntilEndOfTurn,
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            },
        ],
        text: "This Vehicle's power is equal to the number of lands you control.\nWhenever this Vehicle enters or attacks, you may search your library for a basic land card, put it onto the battlefield tapped, then shuffle.\nCrew 4".to_string(),
        // Fully implemented: */4 CDA + enters/attacks fetch + Crew 4 (cap 80d9ab3).
        fully_implemented: true,
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
        assert!(def.fully_implemented); // CDA + enters/attacks fetch + Crew 4 all implemented
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
                Activated {
                    cost: Cost {
                        mana: None,
                        components: [
                            Crew(
                                4,
                            ),
                        ],
                    },
                    effect: BecomeCreature {
                        what: SourceSelf,
                        duration: UntilEndOfTurn,
                    },
                    timing: Instant,
                    restriction: None,
                    is_mana: false,
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
