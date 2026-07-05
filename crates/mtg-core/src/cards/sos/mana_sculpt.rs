//! Mana Sculpt — `{1}{U}{U}` Instant (first printed SOS).
//!
//! Oracle: "Counter target spell. If you control a Wizard, add an amount of {C} equal to the amount of
//! mana spent to cast that spell at the beginning of your next main phase."
//!
//! **Fully implemented** — the lander for **delayed mana** on a new time-based delayed trigger. The
//! effect is a `Sequence`: FIRST a `Conditional` (you control a Wizard) whose `then` is
//! `Effect::AddManaAtNextMainPhase{ Colorless, ManaSpentOfTarget(0) }` — evaluated while the countered
//! spell is still on the stack, so it reads that spell's `mana_spent` and arms a
//! `DelayedTriggerEvent::AtBeginningOfYourNextMainPhase` carrying an `Action::AddMana` — THEN
//! `Effect::Counter` removes the spell (slot 0, so `ManaSpentOfTarget(0)` reads it). At the beginning of
//! the controller's next main phase the delayed trigger fires and the {C} enters their pool.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;
use crate::basics::Zone;

/// grp id (per-set ids live near their cards).
pub const MANA_SCULPT: u32 = 451;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        // "If you control a Wizard, add {C} equal to the mana spent to cast that spell at your next
        // main phase." Armed BEFORE the counter so the still-on-stack spell's mana_spent is readable.
        Effect::Conditional {
            cond: Condition::CountAtLeast {
                zone: Zone::Battlefield,
                filter: CardFilter::HasSubtype(CreatureType::Wizard.into()),
                controller: Some(PlayerRef::Controller),
                n: ValueExpr::Fixed(1),
            },
            then: Box::new(Effect::AddManaAtNextMainPhase {
                who: PlayerRef::Controller,
                color: Color::Colorless,
                amount: ValueExpr::ManaSpentOfTarget(0),
            }),
            otherwise: None,
        },
        // "Counter target spell." (slot 0 — the spell whose mana_spent the clause above reads.)
        Effect::Counter {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::StackObject(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
    ]);
    db.insert(
        spell(
            MANA_SCULPT,
            "Mana Sculpt",
            CardType::Instant,
            Color::Blue,
            mana_cost(1, &[(Color::Blue, 2)]),
            effect,
        )
        .with_text("Counter target spell. If you control a Wizard, add an amount of {C} equal to the amount of mana spent to cast that spell at the beginning of your next main phase."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Phase;
    use crate::cards::sos::ambitious_augmenter::AMBITIOUS_AUGMENTER;
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    #[test]
    fn mana_sculpt_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MANA_SCULPT).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        match def.spell_effect().unwrap() {
            Effect::Sequence(v) => {
                assert!(matches!(&v[0], Effect::Conditional { then, .. } if matches!(**then, Effect::AddManaAtNextMainPhase { .. })));
                assert!(matches!(&v[1], Effect::Counter { .. }));
            }
            other => panic!("expected Sequence, got {other:?}"),
        }
    }

    /// The only legal "target spell" is the Divination (the source Mana Sculpt is excluded from its own
    /// Counter candidates), so slot 0 candidate 0 is it.
    #[derive(Clone)]
    struct CounterAgent;
    impl Agent for CounterAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { .. } => DecisionResponse::Pairs(vec![(0, 0)]),
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn base_game(with_wizard: bool) -> (GameState, ObjId, ObjId) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        // Enough Islands for Divination ({2}{U}) + Mana Sculpt ({1}{U}{U}) = 6.
        for _ in 0..6 {
            state.add_card(PlayerId(0), state.card_db().get(grp::ISLAND).unwrap().chars.clone(), Zone::Battlefield);
        }
        if with_wizard {
            state.add_card(PlayerId(0), state.card_db().get(AMBITIOUS_AUGMENTER).unwrap().chars.clone(), Zone::Battlefield);
        }
        let divination = state.add_card(PlayerId(0), state.card_db().get(grp::DIVINATION).unwrap().chars.clone(), Zone::Hand);
        let sculpt = state.add_card(PlayerId(0), state.card_db().get(MANA_SCULPT).unwrap().chars.clone(), Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        (state, divination, sculpt)
    }

    /// With a Wizard: counter the Divination ({2}{U}, mana spent 3) → the delayed trigger arms, and at
    /// the beginning of the next main phase 3 {C} enter the pool.
    #[test]
    fn counters_and_delays_mana_with_a_wizard() {
        let (state, divination, sculpt) = base_game(true);
        let mut e = Engine::new(state, vec![Box::new(CounterAgent), Box::new(CounterAgent)]);
        e.cast_spell(PlayerId(0), divination, CastVariant::Normal); // Divination on the stack (mana_spent 3)
        e.cast_spell(PlayerId(0), sculpt, CastVariant::Normal); // Mana Sculpt targets it
        e.resolve_top(); // Mana Sculpt resolves: arm delayed mana + counter the Divination
        assert_eq!(e.state.object(divination).zone, Zone::Graveyard, "the spell was countered");
        assert_eq!(e.state.delayed_triggers.len(), 1, "delayed mana armed");

        // Beginning of the controller's next main phase: fire the delayed trigger (the same hook
        // `run_step`'s `PhaseBegan` calls; driven directly so the pool isn't emptied at phase end).
        e.state.phase = Phase::PostcombatMain;
        e.fire_main_phase_delayed_triggers(PlayerId(0));
        assert!(e.state.delayed_triggers.is_empty(), "the delayed trigger fired and was consumed");
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(
            e.state.player(PlayerId(0)).mana_pool.amounts.get(&Color::Colorless).copied().unwrap_or(0),
            3,
            "3 colorless mana (= mana spent on the Divination) entered the pool"
        );
    }

    /// The `run_step` wiring: at the beginning of the controller's next main phase, the real
    /// `PhaseBegan` hook fires and consumes the armed delayed trigger.
    #[test]
    fn delayed_trigger_fires_via_run_step_wiring() {
        let (state, divination, sculpt) = base_game(true);
        let mut e = Engine::new(state, vec![Box::new(CounterAgent), Box::new(CounterAgent)]);
        e.cast_spell(PlayerId(0), divination, CastVariant::Normal);
        e.cast_spell(PlayerId(0), sculpt, CastVariant::Normal);
        e.resolve_top();
        assert_eq!(e.state.delayed_triggers.len(), 1, "armed");
        e.run_step(Phase::PostcombatMain); // its PhaseBegan fires + resolves the delayed mana
        assert!(e.state.delayed_triggers.is_empty(), "consumed at the next main phase via run_step");
    }

    /// Without a Wizard: the spell is still countered, but NO delayed mana is armed.
    #[test]
    fn counters_but_no_mana_without_a_wizard() {
        let (state, divination, sculpt) = base_game(false);
        let mut e = Engine::new(state, vec![Box::new(CounterAgent), Box::new(CounterAgent)]);
        e.cast_spell(PlayerId(0), divination, CastVariant::Normal);
        e.cast_spell(PlayerId(0), sculpt, CastVariant::Normal);
        e.resolve_top();
        assert_eq!(e.state.object(divination).zone, Zone::Graveyard, "still countered");
        assert!(e.state.delayed_triggers.is_empty(), "no Wizard → no delayed mana");
    }
}
