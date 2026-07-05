//! Leech Collector // Bloodletting — `{1}{B}` Creature — Human Warlock 2/2 // `{B}` Sorcery (first
//! printed SOS). A **Prepare** DFC that prepares on your first life gain each turn.
//!
//! Front oracle: "Whenever you gain life for the first time each turn, this creature becomes prepared.
//! (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)"
//! Back oracle (Bloodletting): "Each opponent loses 2 life."
//!
//! **Fully implemented.** The front is a `GainLife` trigger whose condition gates on
//! [`ValueExpr::LifeGainEventsThisTurn`] being **exactly 1** — with `intervening_if: false`, so the
//! condition is evaluated at **trigger-queue time** (CR 603.2c) via the general queue-time condition
//! check added to `queue_self_triggers`. The life-gain-events counter is bumped BEFORE triggers queue,
//! so the first gain reads 1 (fires) and any later gain reads ≥2 (doesn't). The back is
//! [`Effect::LoseLife`] on each opponent.

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::EventPattern;
use crate::effects::condition::Condition;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const LEECH_COLLECTOR: u32 = 409;
/// The copy-only back-face spell (reserved 9700+ Prepare block).
pub const BLOODLETTING: u32 = 9734;

/// "the first life-gain event this turn" — `life_gain_events_this_turn == 1` (exactly, via All/Not).
fn first_life_gain_this_turn() -> Condition {
    let events = || ValueExpr::LifeGainEventsThisTurn { who: PlayerRef::Controller };
    Condition::All(vec![
        Condition::ValueAtLeast(events(), ValueExpr::Fixed(1)),
        Condition::Not(Box::new(Condition::ValueAtLeast(events(), ValueExpr::Fixed(2)))),
    ])
}

pub fn register(db: &mut CardDb) {
    // Back face — "Bloodletting" ({B} Sorcery): each opponent loses 2 life.
    db.insert(
        spell(
            BLOODLETTING,
            "Bloodletting",
            CardType::Sorcery,
            Color::Black,
            mana_cost(0, &[(Color::Black, 1)]),
            Effect::LoseLife { who: PlayerRef::EachOpponent, amount: ValueExpr::Fixed(2) },
        )
        .with_text("Each opponent loses 2 life."),
    );

    // Front face — the 2/2; a `GainLife` trigger gated (queue-time, non-intervening-if) on the first
    // life-gain event this turn → becomes prepared.
    let mut front = creature(
        LEECH_COLLECTOR,
        "Leech Collector",
        &[CreatureType::Human, CreatureType::Warlock],
        Color::Black,
        mana_cost(1, &[(Color::Black, 1)]),
        2,
        2,
        helpers::prepared_abilities(
            BLOODLETTING,
            EventPattern::GainLife,
            Some(first_life_gain_this_turn()),
            false, // non-intervening-if → checked at queue time (event time), the "first time" semantics
        ),
    );
    front.text = "Whenever you gain life for the first time each turn, this creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Bloodletting {B} Sorcery — Each opponent loses 2 life.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::{Phase, Zone};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{ObjId, PlayerId, StackId};
    use crate::priority::Engine;
    use crate::state::GameState;

    fn db_with_card() -> CardDb {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        db
    }

    fn add(state: &mut GameState, who: PlayerId, grp_id: u32, zone: Zone) -> ObjId {
        let c = state.card_db().get(grp_id).unwrap().chars.clone();
        state.add_card(who, c, zone)
    }

    #[test]
    fn leech_collector_ir() {
        let db = db_with_card();
        let front = db.get(LEECH_COLLECTOR).unwrap();
        assert!(matches!(front.abilities[0], crate::effects::ability::Ability::Prepare { spell: BLOODLETTING }));
        match &front.abilities[1] {
            crate::effects::ability::Ability::Triggered { event, intervening_if, condition, .. } => {
                assert_eq!(*event, EventPattern::GainLife);
                assert!(!*intervening_if, "checked at queue time, not resolution");
                assert!(condition.is_some());
            }
            _ => panic!("expected a GainLife triggered ability"),
        }
        let back = db.get(BLOODLETTING).unwrap();
        assert_eq!(back.chars.card_types, vec![CardType::Sorcery]);
    }

    /// Gain life, then process the resulting triggers to completion (mirrors Blech's GainLife test).
    fn gain_and_process(e: &mut Engine, p: PlayerId, amount: i64) {
        e.resolve_effect(
            &Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(amount) },
            &ResolutionCtx { controller: Some(p), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
    }

    /// The FIRST life gain each turn prepares the Leech Collector (queue-time "first time" gate); a
    /// SECOND gain the same turn is not the first event, so it does NOT re-trigger.
    #[test]
    fn first_life_gain_prepares_but_second_does_not() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let leech = add(&mut state, PlayerId(0), LEECH_COLLECTOR, Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);

        // First life gain → the "first time each turn" trigger fires and prepares the Collector.
        gain_and_process(&mut e, PlayerId(0), 1);
        assert!(e.state.object(leech).prepared, "first life gain this turn prepared the Collector");
        assert_eq!(e.state.player(PlayerId(0)).life_gain_events_this_turn, 1);

        // Reset the prepared flag to observe whether a SECOND gain re-triggers (it must not).
        e.state.objects.get_mut(&leech).unwrap().prepared = false;
        gain_and_process(&mut e, PlayerId(0), 1);
        assert_eq!(e.state.player(PlayerId(0)).life_gain_events_this_turn, 2);
        assert!(
            !e.state.object(leech).prepared,
            "a second life gain this turn is not the first event → no trigger"
        );
    }

    /// Bloodletting: each opponent loses 2 life.
    #[test]
    fn bloodletting_each_opponent_loses_two() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let opp_life = state.player(PlayerId(1)).life;
        let effect = state.card_db().get(BLOODLETTING).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(99)),
        );
        assert_eq!(e.state.player(PlayerId(1)).life, opp_life - 2, "the opponent lost 2 life");
    }
}
