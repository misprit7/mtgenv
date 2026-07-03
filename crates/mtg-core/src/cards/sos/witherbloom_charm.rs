//! Witherbloom Charm — `{B}{G}` Instant (first printed SOS).
//!
//! Oracle: "Choose one —
//!   • You may sacrifice a permanent. If you do, draw two cards.
//!   • You gain 5 life.
//!   • Destroy target nonland permanent with mana value 2 or less."
//!
//! **Fully implemented** — a `Modal` "choose one". Mode 1 is `Optional{ IfYouDo{ Sacrifice a
//! permanent, draw two } }` — declining, or sacrificing nothing, draws nothing. Mode 3 destroys one
//! nonland permanent with mana value ≤ 2 (`ManaValue` + `Not(Land)` filter). Multicolored (B/G).

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, Mode};

/// grp id (per-set ids live near their cards).
pub const WITHERBLOOM_CHARM: u32 = 231;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "You may sacrifice a permanent. If you do, draw two cards".to_string(),
                effect: Effect::Optional {
                    prompt: "Sacrifice a permanent to draw two cards?".to_string(),
                    body: Box::new(Effect::IfYouDo {
                        cost: Box::new(Effect::Sacrifice {
                            who: PlayerRef::Controller,
                            what: SelectSpec {
                                zone: Zone::Battlefield,
                                filter: CardFilter::ControlledBy(PlayerRef::Controller),
                                chooser: PlayerRef::Controller,
                                min: ValueExpr::Fixed(1),
                                max: ValueExpr::Fixed(1),
                            },
                        }),
                        reward: Box::new(Effect::Draw {
                            who: PlayerRef::Controller,
                            count: ValueExpr::Fixed(2),
                        }),
                    }),
                },
            },
            Mode {
                label: "You gain 5 life".to_string(),
                effect: Effect::GainLife {
                    who: PlayerRef::Controller,
                    amount: ValueExpr::Fixed(5),
                },
            },
            Mode {
                label: "Destroy target nonland permanent with mana value 2 or less".to_string(),
                effect: Effect::Destroy {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Permanent(CardFilter::All(vec![
                            CardFilter::Not(Box::new(CardFilter::HasCardType(CardType::Land))),
                            CardFilter::ManaValue { min: None, max: Some(2) },
                        ])),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                },
            },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    let mut def = spell(
        WITHERBLOOM_CHARM,
        "Witherbloom Charm",
        CardType::Instant,
        Color::Black,
        mana_cost(0, &[(Color::Black, 1), (Color::Green, 1)]),
        effect,
    )
    .with_text("Choose one —\n• You may sacrifice a permanent. If you do, draw two cards.\n• You gain 5 life.\n• Destroy target nonland permanent with mana value 2 or less.");
    def.chars.colors = vec![Color::Black, Color::Green];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn witherbloom_charm_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(WITHERBLOOM_CHARM).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Black, Color::Green]);
        assert!(def.fully_implemented);
        expect![[r#"
            Modal {
                modes: [
                    Mode {
                        label: "You may sacrifice a permanent. If you do, draw two cards",
                        effect: Optional {
                            prompt: "Sacrifice a permanent to draw two cards?",
                            body: IfYouDo {
                                cost: Sacrifice {
                                    who: Controller,
                                    what: SelectSpec {
                                        zone: Battlefield,
                                        filter: ControlledBy(
                                            Controller,
                                        ),
                                        chooser: Controller,
                                        min: Fixed(
                                            1,
                                        ),
                                        max: Fixed(
                                            1,
                                        ),
                                    },
                                },
                                reward: Draw {
                                    who: Controller,
                                    count: Fixed(
                                        2,
                                    ),
                                },
                            },
                        },
                    },
                    Mode {
                        label: "You gain 5 life",
                        effect: GainLife {
                            who: Controller,
                            amount: Fixed(
                                5,
                            ),
                        },
                    },
                    Mode {
                        label: "Destroy target nonland permanent with mana value 2 or less",
                        effect: Destroy {
                            what: Target(
                                TargetSpec {
                                    kind: Permanent(
                                        All(
                                            [
                                                Not(
                                                    HasCardType(
                                                        Land,
                                                    ),
                                                ),
                                                ManaValue {
                                                    min: None,
                                                    max: Some(
                                                        2,
                                                    ),
                                                },
                                            ],
                                        ),
                                    ),
                                    min: 1,
                                    max: 1,
                                    distinct: true,
                                },
                            ),
                        },
                    },
                ],
                min: 1,
                max: 1,
                allow_repeat: false,
            }"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: mode 0 — sacrificing a permanent draws two; mode 1 — gain 5 life.
    #[test]
    fn witherbloom_charm_modes_resolve() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        #[derive(Clone)]
        struct YesAgent;
        impl Agent for YesAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                    DecisionRequest::SelectCards { min, max, from, .. } => {
                        let n = (*min).max(1).min(*max).min(from.len() as u32);
                        DecisionResponse::Indices((0..n).collect())
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        // Library with two cards to draw; a token-ish creature to sacrifice.
        let mut state = build_game(1, &[&[grp::FOREST, grp::FOREST], &[]]);
        let victim = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let effect = state.card_db().get(WITHERBLOOM_CHARM).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(YesAgent), Box::new(YesAgent)]);
        // Mode 0: sacrifice the creature, draw two.
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_modes: vec![0], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.players[0].graveyard.contains(&victim), "mode 0: sacrificed the creature");
        assert_eq!(e.state.players[0].hand.len(), 2, "mode 0: drew two cards");
        // Mode 1: gain 5 life.
        let life = e.state.player(PlayerId(0)).life;
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_modes: vec![1], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).life, life + 5, "mode 1: gained 5 life");
    }
}
