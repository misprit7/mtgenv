//! Tragedy Feaster — `{2}{B}{B}` Creature — Demon 7/6 (first printed SOS).
//!
//! Oracle: "Trample / Ward—Discard a card. / Infusion — At the beginning of your end step, sacrifice
//! a permanent unless you gained life this turn."
//!
//! **Fully implemented** — the third S17 Ward card (a second Ward—Discard):
//! - **Trample** — a printed keyword.
//! - **Ward—Discard a card** (CR 702.21): `ward_discard()` (see `cards/helpers.rs`).
//! - **Infusion** downside: a `BeginningOfStep(End)` trigger gated on `YourTurn` (so it fires only on
//!   your end step, CR 603.2), whose effect is a `Conditional` on `GainedLifeThisTurn` — if you did
//!   NOT gain life this turn, you `Sacrifice` a permanent of your choice (CR 701.17); if you did,
//!   nothing.

use crate::basics::{Color, Phase, Zone};
use crate::cards::helpers::ward_discard;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const TRAGEDY_FEASTER: u32 = 334;

/// "Infusion — At the beginning of your end step, sacrifice a permanent unless you gained life this
/// turn." Fires on your end step; if you didn't gain life this turn, sacrifice one permanent.
fn infusion_sacrifice() -> Ability {
    Ability::Triggered {
        event: EventPattern::BeginningOfStep(Phase::End),
        // "your end step" — a trigger condition (CR 603.2): only fire on the controller's turn.
        condition: Some(Condition::YourTurn),
        intervening_if: false,
        effect: Effect::Conditional {
            cond: Condition::GainedLifeThisTurn { who: PlayerRef::Controller },
            then: Box::new(Effect::Nothing),
            otherwise: Some(Box::new(Effect::Sacrifice {
                who: PlayerRef::Controller,
                what: SelectSpec {
                    zone: Zone::Battlefield,
                    filter: CardFilter::Any, // "a permanent" — any permanent you control
                    chooser: PlayerRef::Controller,
                    min: ValueExpr::Fixed(1),
                    max: ValueExpr::Fixed(1),
                },
            })),
        },
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        TRAGEDY_FEASTER,
        "Tragedy Feaster",
        &[CreatureType::Demon],
        Color::Black,
        mana_cost(2, &[(Color::Black, 2)]),
        7,
        6,
        vec![ward_discard(), infusion_sacrifice()],
    );
    def.chars.keywords = vec![Keyword::Trample];
    def.text = "Trample\nWard—Discard a card. (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player discards a card.)\nInfusion — At the beginning of your end step, sacrifice a permanent unless you gained life this turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView, SelectReason};
    use crate::basics::Zone;
    use crate::cards::{grp, starter_db};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use expect_test::expect;
    use std::sync::Arc;

    #[test]
    fn tragedy_feaster_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TRAGEDY_FEASTER).unwrap();
        assert_eq!(def.chars.power, Some(7));
        assert_eq!(def.chars.toughness, Some(6));
        assert_eq!(def.chars.keywords, vec![Keyword::Trample]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: BecomesTargeted {
                        filter: ItSelf,
                        by_opponent: true,
                    },
                    condition: None,
                    intervening_if: false,
                    effect: CounterUnlessPay {
                        what: Triggering,
                        cost: Cost {
                            mana: None,
                            components: [
                                Discard(
                                    SelectSpec {
                                        zone: Hand,
                                        filter: Any,
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
                    },
                },
                Triggered {
                    event: BeginningOfStep(
                        End,
                    ),
                    condition: Some(
                        YourTurn,
                    ),
                    intervening_if: false,
                    effect: Conditional {
                        cond: GainedLifeThisTurn {
                            who: Controller,
                        },
                        then: Nothing,
                        otherwise: Some(
                            Sacrifice {
                                who: Controller,
                                what: SelectSpec {
                                    zone: Battlefield,
                                    filter: Any,
                                    chooser: Controller,
                                    min: Fixed(
                                        1,
                                    ),
                                    max: Fixed(
                                        1,
                                    ),
                                },
                            },
                        ),
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// An agent that sacrifices the first offered permanent and passes everything else.
    #[derive(Clone)]
    struct SacAgent;
    impl Agent for SacAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { reason: SelectReason::Sacrifice, from, min, .. } => {
                    DecisionResponse::Indices((0..(*min).min(from.len() as u32)).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Drive Tragedy Feaster's Infusion through the REAL beginning-of-end-step trigger on P0's turn.
    /// Returns P0's battlefield count (before, after the end step). `gained_life` sets the "you gained
    /// life this turn" flag the Infusion is gated on.
    fn run_end_step(gained_life: bool) -> (usize, usize) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        {
            let c = state.card_db().get(TRAGEDY_FEASTER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        // A spare permanent so the sacrifice has a choice other than the Feaster.
        {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        if gained_life {
            state.players[0].life_gained_this_turn = 1;
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::End;
        let before = state.player(PlayerId(0)).battlefield.len();
        let mut e = Engine::new(state, vec![Box::new(SacAgent), Box::new(SacAgent)]);
        // Beginning of P0's end step fires the Infusion trigger (gated on YourTurn), then resolve it.
        e.broadcast(GameEvent::PhaseBegan { turn: 1, phase: Phase::End, active: PlayerId(0) });
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        (before, e.state.player(PlayerId(0)).battlefield.len())
    }

    /// No life gained this turn → the Infusion sacrifice fires; P0 loses one permanent.
    #[test]
    fn infusion_sacrifices_when_no_life_gained() {
        let (before, after) = run_end_step(false);
        assert_eq!(before, 2, "P0 controlled Feaster + Forest before its end step");
        assert_eq!(after, 1, "no life gained → sacrifice a permanent at end step");
    }

    /// Life gained this turn → the "unless" clause holds, so nothing is sacrificed.
    #[test]
    fn infusion_skips_when_life_gained() {
        let (before, after) = run_end_step(true);
        assert_eq!(after, before, "gained life this turn → no sacrifice");
    }
}
