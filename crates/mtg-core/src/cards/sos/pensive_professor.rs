//! Pensive Professor — `{1}{U}{U}` Creature — Human Wizard 0/2 (first printed SOS).
//!
//! Oracle: "Increment (Whenever you cast a spell, if the amount of mana you spent is greater than
//! this creature's power or toughness, put a +1/+1 counter on this creature.) / Whenever one or more
//! +1/+1 counters are put on this creature, draw a card."
//!
//! **Fully implemented** — the shared `increment_ability()` (S6) plus a `CountersPutOnSelf { +1/+1 }`
//! trigger (the new "counters put on self" `EventPattern`, fired once per counter-adding event by the
//! `Action::AddCounters` executor via `GameEvent::CountersPut`) whose effect is `Draw 1`. Increment's
//! own +1/+1 injection fires this trigger, so a big spell draws a card.

use crate::basics::{Color, CounterKind};
use crate::cards::helpers::increment_ability;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const PENSIVE_PROFESSOR: u32 = 348;

/// "Whenever one or more +1/+1 counters are put on this creature, draw a card."
fn draw_on_counter() -> Ability {
    Ability::Triggered {
        event: EventPattern::CountersPutOnSelf { kind: CounterKind::PlusOnePlusOne },
        condition: None,
        intervening_if: false,
        effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        PENSIVE_PROFESSOR,
        "Pensive Professor",
        &[CreatureType::Human, CreatureType::Wizard],
        Color::Blue,
        mana_cost(1, &[(Color::Blue, 2)]),
        0,
        2,
        vec![increment_ability(), draw_on_counter()],
    );
    def.text = "Increment (Whenever you cast a spell, if the amount of mana you spent is greater than this creature's power or toughness, put a +1/+1 counter on this creature.)\nWhenever one or more +1/+1 counters are put on this creature, draw a card.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::Zone;
    use crate::cards::{grp, starter_db};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::effects::EffectTarget;
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    /// Putting a +1/+1 counter on the Professor fires its `CountersPutOnSelf` trigger through the REAL
    /// engine (broadcast → collect_triggers → stack → resolve), drawing one card.
    #[test]
    fn drawing_when_a_counter_is_put_on_it() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let prof = {
            let c = state.card_db().get(PENSIVE_PROFESSOR).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // Two cards to draw from.
        for _ in 0..2 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library);
        }
        state.active_player = PlayerId(0);
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let hand_before = e.state.player(PlayerId(0)).hand.len();
        // A real PutCounters on the Professor → AddCounters → GameEvent::CountersPut → the trigger queues.
        e.resolve_effect(
            &Effect::PutCounters {
                what: EffectTarget::SourceSelf,
                kind: CounterKind::PlusOnePlusOne,
                n: ValueExpr::Fixed(1),
            },
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(prof), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        // Drain the queued trigger onto the stack and resolve it.
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(e.state.object(prof).counters.get(&CounterKind::PlusOnePlusOne), 1, "got a +1/+1");
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand_before + 1, "drew a card from the trigger");
    }

    /// A -1/-1 (or other) counter does NOT fire the +1/+1-specific trigger.
    #[test]
    fn no_draw_on_a_different_counter_kind() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let prof = {
            let c = state.card_db().get(PENSIVE_PROFESSOR).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        for _ in 0..2 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library);
        }
        state.active_player = PlayerId(0);
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let hand_before = e.state.player(PlayerId(0)).hand.len();
        e.resolve_effect(
            &Effect::PutCounters {
                what: EffectTarget::SourceSelf,
                kind: CounterKind::MinusOneMinusOne,
                n: ValueExpr::Fixed(1),
            },
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(prof), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand_before, "no draw for a -1/-1 counter");
    }
}
