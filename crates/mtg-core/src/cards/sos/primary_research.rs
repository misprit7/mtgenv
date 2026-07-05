//! Primary Research — `{4}{W}` Enchantment (first printed SOS).
//!
//! Oracle: "When this enchantment enters, return target nonland permanent card with mana value 3 or
//! less from your graveyard to the battlefield. / At the beginning of your end step, if a card left
//! your graveyard this turn, draw a card."
//!
//! **Fully implemented** — an ETB reanimation (`MoveZone` of a targeted nonland permanent card,
//! MV ≤ 3, from your graveyard to the battlefield) plus a "your end step, if a card left your
//! graveyard this turn, draw" trigger (`CardLeftGraveyardThisTurn` gate). The ETB reanimation itself
//! satisfies that gate (the reanimated card left the graveyard), so it naturally draws that turn.

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos, Phase};
use crate::cards::{enchantment, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const PRIMARY_RESEARCH: u32 = 269;

fn nonland_permanent_mv3_in_graveyard() -> TargetSpec {
    TargetSpec {
        kind: TargetKind::CardInZone {
            zone: Zone::Graveyard,
            filter: CardFilter::All(vec![
                CardFilter::ManaValue { min: None, max: Some(3) },
                CardFilter::AnyOf(vec![
                    CardFilter::HasCardType(CardType::Creature),
                    CardFilter::HasCardType(CardType::Artifact),
                    CardFilter::HasCardType(CardType::Enchantment),
                    CardFilter::HasCardType(CardType::Planeswalker),
                ]),
            ]),
        },
        min: 1,
        max: 1,
        distinct: true,
    }
}

pub fn register(db: &mut CardDb) {
    db.insert(
        enchantment(
            PRIMARY_RESEARCH,
            "Primary Research",
            Color::White,
            mana_cost(4, &[(Color::White, 1)]),
            vec![
                Ability::Triggered {
                    event: EventPattern::SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Effect::MoveZone {
                        what: EffectTarget::Target(nonland_permanent_mv3_in_graveyard()),
                        to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
                        tapped: false,
                    },
                },
                Ability::Triggered {
                    event: EventPattern::BeginningOfStep(Phase::End),
                    condition: Some(Condition::All(vec![
                        Condition::YourTurn,
                        Condition::CardLeftGraveyardThisTurn { who: PlayerRef::Controller },
                    ])),
                    intervening_if: true,
                    effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
                },
            ],
        )
        .with_text("When this enchantment enters, return target nonland permanent card with mana value 3 or less from your graveyard to the battlefield.\nAt the beginning of your end step, if a card left your graveyard this turn, draw a card."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn primary_research_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        assert!(db.get(PRIMARY_RESEARCH).unwrap().fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: MoveZone {
                        what: Target(
                            TargetSpec {
                                kind: CardInZone {
                                    zone: Graveyard,
                                    filter: All(
                                        [
                                            ManaValue {
                                                min: None,
                                                max: Some(
                                                    3,
                                                ),
                                            },
                                            AnyOf(
                                                [
                                                    HasCardType(
                                                        Creature,
                                                    ),
                                                    HasCardType(
                                                        Artifact,
                                                    ),
                                                    HasCardType(
                                                        Enchantment,
                                                    ),
                                                    HasCardType(
                                                        Planeswalker,
                                                    ),
                                                ],
                                            ),
                                        ],
                                    ),
                                },
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        to: ZoneDest {
                            zone: Battlefield,
                            pos: Any,
                        },
                        tapped: false,
                    },
                },
                Triggered {
                    event: BeginningOfStep(
                        End,
                    ),
                    condition: Some(
                        All(
                            [
                                YourTurn,
                                CardLeftGraveyardThisTurn {
                                    who: Controller,
                                },
                            ],
                        ),
                    ),
                    intervening_if: true,
                    effect: Draw {
                        who: Controller,
                        count: Fixed(
                            1,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", db.get(PRIMARY_RESEARCH).unwrap().abilities));
    }

    /// Behaviour: the ETB reanimates a creature from the graveyard, which sets "a card left your
    /// graveyard this turn" — so the end-step draw condition then holds.
    #[test]
    fn primary_research_reanimates_and_marks_graveyard_departure() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Graveyard);
        let etb = match &state.card_db().get(PRIMARY_RESEARCH).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected ETB Triggered, got {o:?}"),
        };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        assert_eq!(e.state.player(PlayerId(0)).cards_left_graveyard_this_turn, 0);
        e.resolve_effect(&etb, &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(bears)], ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert!(e.state.players[0].battlefield.contains(&bears), "reanimated onto the battlefield");
        assert_eq!(e.state.player(PlayerId(0)).cards_left_graveyard_this_turn, 1, "a card left the graveyard");
        // The end-step draw condition now holds.
        assert!(crate::conditions::holds_for_source(
            &e.state,
            &Condition::CardLeftGraveyardThisTurn { who: PlayerRef::Controller },
            PlayerId(0),
            None,
        ));
    }

    /// Integration (real turn engine): the end-step draw fires only when a card left your graveyard
    /// this turn — proving the begin-of-step trigger is queued and its `intervening_if` evaluated.
    #[test]
    fn primary_research_end_step_draw_fires_only_after_a_graveyard_leave() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
        use crate::basics::{Phase, Zone};
        use crate::cards::{build_game, grp};
        use crate::ids::PlayerId;
        use crate::priority::Engine;

        #[derive(Clone)]
        struct PassAgent;
        impl Agent for PassAgent {
            fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        let run = |left_gy: bool| -> usize {
            let mut state = build_game(1, &[&[grp::FOREST, grp::FOREST], &[]]);
            let c = state.card_db().get(PRIMARY_RESEARCH).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
            if left_gy {
                state.player_mut(PlayerId(0)).cards_left_graveyard_this_turn = 1;
            }
            state.active_player = PlayerId(0);
            state.phase = Phase::End;
            let hand_before = state.player(PlayerId(0)).hand.len();
            let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
            e.broadcast(GameEvent::PhaseBegan { turn: 1, phase: Phase::End, active: PlayerId(0) });
            e.run_agenda();
            if !e.state.stack.is_empty() {
                e.resolve_top();
            }
            e.state.player(PlayerId(0)).hand.len() - hand_before
        };

        assert_eq!(run(true), 1, "a card left the graveyard → intervening-if holds → draw");
        assert_eq!(run(false), 0, "nothing left the graveyard → no draw");
    }
}
