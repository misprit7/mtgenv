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
use crate::subtypes::LandType;

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
                        one_of: None,
                        restriction: None,
                    },
                },
                timing: Timing::Instant,
                restriction: Some(Restriction::OnlyIf(Condition::CountAtLeast {
                    zone: Zone::Battlefield,
                    filter: CardFilter::AnyOf(vec![
                        CardFilter::HasSubtype(LandType::Forest.into()),
                        CardFilter::HasSubtype(LandType::Plains.into()),
                    ]),
                    controller: Some(PlayerRef::Controller),
                    n: ValueExpr::Fixed(1),
                })),
                is_mana: true,
            },
        ],
        text: "{T}: Add {G}.\n{T}: Add {W}. Activate only if you control a Forest or a Plains."
            .to_string(),
        fully_implemented: true,
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
        assert!(def.is_mana_source()); // mana is first-class IR (authored {T}: Add … abilities)
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
                                    White,
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
                    restriction: Some(
                        OnlyIf(
                            CountAtLeast {
                                zone: Battlefield,
                                filter: AnyOf(
                                    [
                                        HasSubtype(
                                            Land(
                                                Forest,
                                            ),
                                        ),
                                        HasSubtype(
                                            Land(
                                                Plains,
                                            ),
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

    /// Behaviour: resolving the unconditional `{T}: Add {G}` mana ability adds one green to your pool.
    /// (The conditional `{W}` is an *activation* gate — `Restriction::OnlyIf` — covered by the IR test.)
    #[test]
    fn hushwood_taps_for_green() {
        use crate::agent::RandomAgent;
        use crate::basics::{Color, Zone};
        use crate::cards::build_game;
        use crate::effects::ability::Ability;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(HUSHWOOD_VERGE).unwrap().chars.clone();
        let verge = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let mana = match &state.card_db().get(HUSHWOOD_VERGE).unwrap().abilities[0] {
            Ability::Activated { effect, .. } => effect.clone(),
            o => panic!("expected {{G}} mana Activated, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &mana,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(verge), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.players[0].mana_pool.amounts.get(&Color::Green), Some(&1));
    }

    /// #60 end-to-end (the REAL affordability/activation gate): "{T}: Add {W}. Activate only if you
    /// control a Forest or a Plains." We probe it through `legal_actions`: a `{W}` spell (Erode) is
    /// castable iff Hushwood can make white — i.e. iff you control a Forest/Plains. A **Forest** is the
    /// clean isolator: it doesn't produce white itself, it only *enables* Hushwood's `{W}`. So with
    /// Hushwood alone, Erode is NOT offered; add a Forest and it IS. (Mana abilities aren't surfaced in
    /// `legal_actions` directly — CR 605 no-stack path — so the spell-affordability gate is the probe.)
    #[test]
    fn hushwood_white_ability_gated_on_controlling_a_forest_or_plains() {
        use crate::agent::{PlayableAction, RandomAgent};
        use crate::basics::{Phase, Zone};
        use crate::cards::sos::erode::ERODE;
        use crate::cards::{grp, starter_db};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        // Whether P0 may cast Erode ({W}), given whether a Forest is also in play to enable Hushwood's {W}.
        let can_cast_erode = |with_forest: bool| -> bool {
            let mut state = GameState::new(2, 1);
            state.set_card_db(Arc::new(starter_db()));
            {
                let c = state.card_db().get(HUSHWOOD_VERGE).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield);
            }
            if with_forest {
                let c = state.card_db().get(grp::FOREST).unwrap().chars.clone(); // enables {W}, makes only {G}
                state.add_card(PlayerId(0), c, Zone::Battlefield);
            }
            let erode = {
                let c = state.card_db().get(ERODE).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Hand)
            };
            // A victim so Erode has a legal target (affordability + targeting are both pre-checked).
            {
                let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
                state.add_card(PlayerId(1), c, Zone::Battlefield);
            }
            state.active_player = PlayerId(0);
            state.phase = Phase::PrecombatMain;
            let e = Engine::new(
                state,
                vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
            );
            e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::Cast { spell, .. } if *spell == erode))
        };

        assert!(!can_cast_erode(false), "Hushwood alone can't make {{W}} (no Forest/Plains) → Erode uncastable");
        assert!(can_cast_erode(true), "a Forest enables Hushwood's {{W}} → Erode castable");
    }
}
