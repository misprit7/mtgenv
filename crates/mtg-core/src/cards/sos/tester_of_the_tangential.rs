//! Tester of the Tangential — `{1}{U}` Creature — Djinn Wizard 1/1.
//!
//! Oracle: "Increment (Whenever you cast a spell, if the amount of mana you spent is greater than this
//! creature's power or toughness, put a +1/+1 counter on this creature.)
//! At the beginning of combat on your turn, you may pay {X}. When you do, move X +1/+1 counters from
//! this creature onto another target creature."
//!
//! **Fully implemented** — the shared `helpers::increment_ability()` keyword + a begin-combat trigger
//! `MayPayCost{ cost: {X}, then: MoveCounters{ SourceSelf → another target creature, count: X } }`. The
//! new `MayPayCost`-with-`{X}` announces X (bounded by affordable mana; X = 0 declines) and threads it
//! into the reward as `ValueExpr::X`; `Effect::MoveCounters` moves that many +1/+1 counters, capped at
//! what Tester actually has.
//!
//! ⚠️ Timing caveat: the "another target creature" is chosen when the trigger goes on the stack (a beat
//! before the pay decision) rather than reflexively after paying (CR 603.7c). Observably equivalent for
//! the pool — a declined pay (X = 0) simply moves nothing.

use crate::basics::{CounterKind, Color, Phase};
use crate::cards::helpers::increment_ability;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const TESTER_OF_THE_TANGENTIAL: u32 = 438;

pub fn register(db: &mut CardDb) {
    // A pure `{X}` mana cost for the "you may pay {X}" clause.
    let mut x_cost = mana_cost(0, &[]);
    x_cost.x = 1;
    // "At the beginning of combat on your turn, you may pay {X}. When you do, move X +1/+1 counters
    // from this creature onto another target creature."
    let combat = Ability::Triggered {
        event: EventPattern::BeginningOfStep(Phase::BeginCombat),
        condition: Some(Condition::YourTurn),
        intervening_if: false,
        effect: Effect::MayPayCost {
            cost: Cost { mana: Some(x_cost), components: vec![] },
            then: Box::new(Effect::MoveCounters {
                from: EffectTarget::SourceSelf,
                to: EffectTarget::Target(TargetSpec {
                    kind: TargetKind::Creature(CardFilter::Not(Box::new(CardFilter::ItSelf))),
                    min: 1,
                    max: 1,
                    distinct: true,
                }),
                kind: CounterKind::PlusOnePlusOne,
                count: ValueExpr::X,
            }),
        },
    };
    let mut def = creature(
        TESTER_OF_THE_TANGENTIAL,
        "Tester of the Tangential",
        &[CreatureType::Djinn, CreatureType::Wizard],
        Color::Blue,
        mana_cost(1, &[(Color::Blue, 1)]),
        1,
        1,
        vec![increment_ability(), combat],
    );
    def.text = "Increment (Whenever you cast a spell, if the amount of mana you spent is greater than this creature's power or toughness, put a +1/+1 counter on this creature.)\nAt the beginning of combat on your turn, you may pay {X}. When you do, move X +1/+1 counters from this creature onto another target creature.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView, RandomAgent};
    use crate::basics::{Target, Zone};
    use crate::cards::{grp, starter_db};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{ObjId, PlayerId, StackId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn tester_shape() {
        let db = db_with_card();
        let def = db.get(TESTER_OF_THE_TANGENTIAL).unwrap();
        assert_eq!((def.chars.power, def.chars.toughness), (Some(1), Some(1)));
        assert_eq!(def.chars.colors, vec![Color::Blue]);
        assert!(def.fully_implemented);
        assert!(matches!(
            def.abilities[1],
            Ability::Triggered { event: EventPattern::BeginningOfStep(Phase::BeginCombat), .. }
        ));
    }

    /// Answers ChooseNumber (X) with a fixed value; passes everything else.
    #[derive(Clone)]
    struct XAgent(i64);
    impl Agent for XAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseNumber { .. } => DecisionResponse::Number(self.0),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Build P0 with Tester (carrying `counters` +1/+1 counters), another creature, and `lands` Islands
    /// for mana. Resolve the combat ability directly, targeting the other creature, paying X = `pay_x`.
    fn run(counters: u32, pay_x: i64, lands: usize) -> (Engine, ObjId, ObjId) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let tester = {
            let c = state.card_db().get(TESTER_OF_THE_TANGENTIAL).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        if counters > 0 {
            if let Some(o) = state.objects.get_mut(&tester) {
                o.counters.counts.insert(CounterKind::PlusOnePlusOne, counters);
            }
        }
        let other = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        for _ in 0..lands {
            let c = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.mark_chars_dirty();
        let combat = match &state.card_db().get(TESTER_OF_THE_TANGENTIAL).unwrap().abilities[1] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected combat ability, got {o:?}"),
        };
        let mut e = Engine::new(state, vec![Box::new(XAgent(pay_x)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &combat,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                source: Some(tester),
                chosen_targets: vec![Target::Object(other)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        (e, tester, other)
    }

    fn p1p1(e: &Engine, o: ObjId) -> u32 {
        e.state.object(o).counters.get(&CounterKind::PlusOnePlusOne)
    }

    /// Pay X = 2 with two Islands: two counters move from Tester (2 → 0) onto the other creature (0 → 2).
    #[test]
    fn paying_x_moves_x_counters() {
        let (e, tester, other) = run(2, 2, 2);
        assert_eq!(p1p1(&e, tester), 0, "Tester lost its two counters");
        assert_eq!(p1p1(&e, other), 2, "the other creature gained two");
    }

    /// Pay X = 3 but Tester only has 2 counters (and 3 mana): the move caps at what's present — 2.
    #[test]
    fn moving_more_than_present_moves_all() {
        let (e, tester, other) = run(2, 3, 3);
        assert_eq!(p1p1(&e, tester), 0, "Tester had only two to give");
        assert_eq!(p1p1(&e, other), 2, "so only two moved");
    }

    /// Decline (X = 0): nothing moves.
    #[test]
    fn declining_moves_nothing() {
        let (e, tester, other) = run(2, 0, 2);
        assert_eq!(p1p1(&e, tester), 2, "Tester keeps its counters");
        assert_eq!(p1p1(&e, other), 0, "nothing moved");
    }
}
