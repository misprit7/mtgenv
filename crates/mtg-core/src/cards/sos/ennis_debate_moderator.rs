//! Ennis, Debate Moderator — `{1}{W}` Legendary Creature — Human Cleric 1/1.
//!
//! Oracle: "When Ennis enters, exile up to one other target creature you control. Return that card to
//! the battlefield under its owner's control at the beginning of the next end step.
//! At the beginning of your end step, if one or more cards were put into exile this turn, put a +1/+1
//! counter on Ennis."
//!
//! **Fully implemented** — the ETB is the shared `Effect::ExileReturnNextEndStep` timed-blink (up to one
//! OTHER creature you control, `Not(ItSelf) ∧ ControlledBy(Controller)`). The end-step trigger uses the
//! new **cards-put-into-exile-this-turn tracker**: `Player.cards_exiled_this_turn` (incremented in
//! `move_object` on any move to exile, reset each turn) summed by `ValueExpr::CardsExiledThisTurn`.

use crate::basics::{Color, CounterKind, Phase};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const ENNIS_DEBATE_MODERATOR: u32 = 442;

pub fn register(db: &mut CardDb) {
    // "When Ennis enters, exile up to one other target creature you control; return it next end step."
    let etb = Ability::Triggered {
        event: EventPattern::SelfEnters,
        condition: None,
        intervening_if: false,
        effect: Effect::ExileReturnNextEndStep {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::All(vec![
                    CardFilter::Not(Box::new(CardFilter::ItSelf)),
                    CardFilter::ControlledBy(PlayerRef::Controller),
                ])),
                min: 0,
                max: 1,
                distinct: true,
            }),
        },
    };
    // "At the beginning of your end step, if one or more cards were put into exile this turn, +1/+1."
    let end_step = Ability::Triggered {
        event: EventPattern::BeginningOfStep(Phase::End),
        condition: Some(Condition::All(vec![
            Condition::YourTurn,
            Condition::ValueAtLeast(ValueExpr::CardsExiledThisTurn, ValueExpr::Fixed(1)),
        ])),
        intervening_if: true,
        effect: Effect::PutCounters {
            what: EffectTarget::SourceSelf,
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::Fixed(1),
        },
    };
    let mut def = creature(
        ENNIS_DEBATE_MODERATOR,
        "Ennis, Debate Moderator",
        &[CreatureType::Human, CreatureType::Cleric],
        Color::White,
        mana_cost(1, &[(Color::White, 1)]),
        1,
        1,
        vec![etb, end_step],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.text = "When Ennis enters, exile up to one other target creature you control. Return that card to the battlefield under its owner's control at the beginning of the next end step.\nAt the beginning of your end step, if one or more cards were put into exile this turn, put a +1/+1 counter on Ennis.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
    use crate::basics::{Target, Zone};
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn ennis_shape() {
        let db = db_with_card();
        let def = db.get(ENNIS_DEBATE_MODERATOR).unwrap();
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(1), Some(1)));
        assert!(def.fully_implemented);
    }

    /// Optionally exiles the named creature for the ETB (`exile`), else declines.
    #[derive(Clone)]
    struct EnnisAgent {
        exile_target: Option<ObjId>,
    }
    impl Agent for EnnisAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => match self.exile_target {
                    Some(t) => match slots[0].legal.iter().position(|x| *x == Target::Object(t)) {
                        Some(i) => DecisionResponse::Pairs(vec![(0, i as u32)]),
                        None => DecisionResponse::Pairs(vec![]),
                    },
                    None => DecisionResponse::Pairs(vec![]),
                },
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn drive(e: &mut Engine) {
        loop {
            e.run_agenda();
            if e.state.stack.items.is_empty() {
                break;
            }
            e.resolve_top();
        }
    }

    /// Set up Ennis + a Bears on P0's battlefield. Fire Ennis's ETB (exiling the Bears if `exile`), then
    /// the end step. Returns (engine, ennis, bears).
    fn run(exile: bool) -> (Engine, ObjId, ObjId) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let ennis = {
            let c = state.card_db().get(ENNIS_DEBATE_MODERATOR).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.active_player = PlayerId(0);
        state.mark_chars_dirty();
        let target = if exile { Some(bears) } else { None };
        let mut e = Engine::new(
            state,
            vec![Box::new(EnnisAgent { exile_target: target }), Box::new(EnnisAgent { exile_target: target })],
        );
        // Fire Ennis's ETB.
        e.broadcast(GameEvent::ObjectMoved { obj: ennis, to: Zone::Battlefield });
        drive(&mut e);
        // Now the end step.
        e.state.phase = Phase::End;
        e.broadcast(GameEvent::PhaseBegan { turn: 1, phase: Phase::End, active: PlayerId(0) });
        drive(&mut e);
        (e, ennis, bears)
    }

    /// ETB exiles the Bears → a card was exiled this turn → Ennis gets a +1/+1 at the end step (and the
    /// Bears returns).
    #[test]
    fn exiling_this_turn_grows_ennis_at_end_step() {
        let (e, ennis, bears) = run(true);
        assert_eq!(
            e.state.object(ennis).counters.get(&CounterKind::PlusOnePlusOne),
            1,
            "Ennis grew because a card was exiled this turn"
        );
        assert_eq!(e.state.object(bears).zone, Zone::Battlefield, "the blinked Bears returned");
    }

    /// ETB exiles nothing and no other exile happens → no counter at the end step.
    #[test]
    fn no_exile_no_counter() {
        let (e, ennis, _) = run(false);
        assert_eq!(
            e.state.object(ennis).counters.get(&CounterKind::PlusOnePlusOne),
            0,
            "nothing was exiled → no counter"
        );
    }
}
