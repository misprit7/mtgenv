//! Ambitious Augmenter — `{G}` Creature — Turtle Wizard 1/1.
//!
//! Oracle: "Increment (Whenever you cast a spell, if the amount of mana you spent is greater than this
//! creature's power or toughness, put a +1/+1 counter on this creature.)
//! When this creature dies, if it had one or more counters on it, create a 0/0 green and blue Fractal
//! creature token, then put this creature's counters on that token."
//!
//! **Fully implemented** — the shared `helpers::increment_ability()` (an existing keyword) plus a
//! `SelfDies` trigger that creates a Fractal entering with the creature's last-known +1/+1 counter
//! count. The dies clause reads `ValueExpr::CountersOnSelf` — now backed by the last-known counter bag
//! (CR 603.10a) captured when the creature left the battlefield — as the Fractal's `dynamic_counters`,
//! so "put this creature's counters on that token" falls out of creating the token with them. Gated on
//! "if it had one or more counters" so a counterless death makes no doomed 0/0.

use crate::basics::{Color, CounterKind};
use crate::cards::helpers::{fractal_token, increment_ability};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const AMBITIOUS_AUGMENTER: u32 = 436;

pub fn register(db: &mut CardDb) {
    let dies = Ability::Triggered {
        event: EventPattern::SelfDies,
        // "if it had one or more counters on it" (intervening-if on the last-known +1/+1 count).
        condition: Some(Condition::ValueAtLeast(
            ValueExpr::CountersOnSelf(CounterKind::PlusOnePlusOne),
            ValueExpr::Fixed(1),
        )),
        intervening_if: true,
        // "create a 0/0 Fractal, then put this creature's counters on that token" — the token enters
        // with that many +1/+1 counters (dynamic_counters read the last-known count).
        effect: Effect::CreateToken {
            spec: fractal_token(0),
            count: ValueExpr::Fixed(1),
            controller: PlayerRef::Controller,
            dynamic_counters: vec![(
                CounterKind::PlusOnePlusOne,
                ValueExpr::CountersOnSelf(CounterKind::PlusOnePlusOne),
            )],
        },
    };
    let mut def = creature(
        AMBITIOUS_AUGMENTER,
        "Ambitious Augmenter",
        &[CreatureType::Turtle, CreatureType::Wizard],
        Color::Green,
        mana_cost(0, &[(Color::Green, 1)]),
        1,
        1,
        vec![increment_ability(), dies],
    );
    def.text = "Increment (Whenever you cast a spell, if the amount of mana you spent is greater than this creature's power or toughness, put a +1/+1 counter on this creature.)\nWhen this creature dies, if it had one or more counters on it, create a 0/0 green and blue Fractal creature token, then put this creature's counters on that token.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::{Phase, Zone};
    use crate::cards::starter_db;
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
    fn augmenter_shape() {
        let db = db_with_card();
        let def = db.get(AMBITIOUS_AUGMENTER).unwrap();
        assert_eq!((def.chars.power, def.chars.toughness), (Some(1), Some(1)));
        assert_eq!(def.chars.colors, vec![Color::Green]);
        assert!(def.fully_implemented);
        assert!(matches!(def.abilities[1], Ability::Triggered { event: EventPattern::SelfDies, .. }));
    }

    /// Mark lethal damage and settle SBAs — a REAL death (move off the battlefield captures LKI, fires
    /// the dies trigger), so the dies clause reads the last-known counter count.
    fn kill_and_settle(e: &mut Engine, obj: ObjId) {
        let t = e.state.computed(obj).toughness.unwrap_or(1).max(1) as u32;
        e.state.objects.get_mut(&obj).unwrap().damage_marked = t;
        e.state.mark_chars_dirty();
        e.run_agenda();
        while !e.state.stack.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
    }

    /// Give the Augmenter `counters` +1/+1 counters, then kill it via lethal damage: a Fractal token
    /// enters carrying that many +1/+1 counters. With zero counters, no token is made.
    fn run_dies(counters: u32) -> Engine {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let aug = {
            let c = state.card_db().get(AMBITIOUS_AUGMENTER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        if counters > 0 {
            if let Some(o) = state.objects.get_mut(&aug) {
                o.counters.counts.insert(CounterKind::PlusOnePlusOne, counters);
            }
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        state.mark_chars_dirty();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        kill_and_settle(&mut e, aug);
        e
    }

    fn fractals(e: &Engine) -> Vec<crate::ids::ObjId> {
        e.state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .copied()
            .filter(|&o| e.state.object(o).chars.name == "Fractal")
            .collect()
    }

    #[test]
    fn dies_with_counters_makes_a_fractal_carrying_them() {
        let e = run_dies(2);
        let fs = fractals(&e);
        assert_eq!(fs.len(), 1, "one Fractal token created");
        assert_eq!(
            e.state.object(fs[0]).counters.get(&CounterKind::PlusOnePlusOne),
            2,
            "the Fractal carries the Augmenter's two +1/+1 counters"
        );
    }

    #[test]
    fn dies_without_counters_makes_no_token() {
        let e = run_dies(0);
        assert!(fractals(&e).is_empty(), "no counters → no Fractal");
    }
}
