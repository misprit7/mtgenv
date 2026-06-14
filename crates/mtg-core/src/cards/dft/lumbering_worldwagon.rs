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

    /// Behaviour: the `*`/4 CDA (layer 7a) computes power = the number of lands **you** control,
    /// via the public `GameState::computed` — a real layer-system check, not just IR shape.
    #[test]
    fn lumbering_power_equals_lands_you_control() {
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::ids::PlayerId;
        let mut state = build_game(1, &[&[], &[]]);
        let wagon_chars = state.card_db().get(LUMBERING_WORLDWAGON).unwrap().chars.clone();
        let forest_chars = state.card_db().get(grp::FOREST).unwrap().chars.clone();
        let wagon = state.add_card(PlayerId(0), wagon_chars, Zone::Battlefield);
        // No lands yet → power 0; toughness fixed 4.
        assert_eq!(state.computed(wagon).power, Some(0));
        assert_eq!(state.computed(wagon).toughness, Some(4));
        // Three Forests you control → power 3.
        for _ in 0..3 {
            state.add_card(PlayerId(0), forest_chars.clone(), Zone::Battlefield);
        }
        assert_eq!(state.computed(wagon).power, Some(3));
        // A land an opponent controls is NOT counted (controller-scoped count).
        state.add_card(PlayerId(1), forest_chars.clone(), Zone::Battlefield);
        assert_eq!(state.computed(wagon).power, Some(3));
    }

    /// Behaviour: resolving Crew 4 (the `BecomeCreature` effect) animates the Vehicle into an artifact
    /// creature — it keeps its artifact type and gains the creature type.
    #[test]
    fn lumbering_crew_animates_the_vehicle() {
        use crate::agent::RandomAgent;
        use crate::basics::{CardType, Zone};
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let wagon_chars = state.card_db().get(LUMBERING_WORLDWAGON).unwrap().chars.clone();
        let wagon = state.add_card(PlayerId(0), wagon_chars, Zone::Battlefield);
        assert!(!state.computed(wagon).is_creature(), "a Vehicle is not a creature until crewed");
        // The Crew ability's effect = BecomeCreature(self, until EOT).
        let crew = match &state.card_db().get(LUMBERING_WORLDWAGON).unwrap().abilities[3] {
            Ability::Activated { effect, .. } => effect.clone(),
            other => panic!("expected Crew Activated, got {other:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &crew,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                source: Some(wagon),
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        let cc = e.state.computed(wagon);
        assert!(cc.is_creature(), "crewed → an artifact creature");
        assert!(cc.card_types.contains(&CardType::Artifact), "still an artifact");
    }

    /// #60 end-to-end (the REAL activate path): Crew 4 via `activate_ability` actually **pays the crew
    /// cost** — taps untapped creatures you control with total power ≥ 4 (CR 702.122) — which the
    /// resolve-level test above skips. Two 2/2s (total power 4) crew the Vehicle: both end **tapped**
    /// and the Worldwagon becomes an artifact *creature* until end of turn (keeping its `*`/4 CDA).
    #[test]
    fn lumbering_crew_via_full_activation_taps_creatures() {
        use crate::agent::{AbilityRef, Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{CardType, Zone};
        use crate::cards::{grp, starter_db};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        // Passive: pay_crew auto-fills the crew from candidates when the agent under-selects.
        #[derive(Clone)]
        struct PassiveAgent;
        impl Agent for PassiveAgent {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let wagon = {
            let c = state.card_db().get(LUMBERING_WORLDWAGON).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let bears: Vec<_> = (0..2)
            .map(|_| {
                let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(); // 2/2
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            })
            .collect();
        let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
        assert!(!e.state.computed(wagon).is_creature(), "not a creature before crewing");
        e.activate_ability(PlayerId(0), wagon, AbilityRef(3)); // Crew 4: taps creatures totaling power ≥ 4
        e.resolve_top(); // BecomeCreature resolves

        assert!(
            bears.iter().all(|&b| e.state.object(b).status.tapped),
            "both 2/2s were tapped to pay Crew 4 (total power 4)"
        );
        let cc = e.state.computed(wagon);
        assert!(cc.is_creature(), "crewed → an artifact creature");
        assert!(cc.card_types.contains(&CardType::Artifact), "still an artifact");
        assert_eq!(cc.toughness, Some(4), "keeps its */4 toughness");
    }
}
