//! Sazh's Chocobo — `{G}` Creature — Bird 0/1 (first printed FIN, Final Fantasy).
//!
//! "Landfall — Whenever a land you control enters, put a +1/+1 counter on this creature."
//! Fully implemented: a triggered ability on the landfall event (a land you control entering,
//! C4) putting a fixed +1/+1 counter on itself (C2). No deferred clauses.

use crate::basics::{Color, CounterKind};
use crate::cards::helpers::land_you_control;
use crate::cards::{creature, mana_cost, CardDb};
use crate::subtypes::CreatureType;
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const SAZHS_CHOCOBO: u32 = 102;

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            SAZHS_CHOCOBO,
            "Sazh's Chocobo",
            &[CreatureType::Bird],
            Color::Green,
            mana_cost(0, &[(Color::Green, 1)]),
            0,
            1,
            vec![Ability::Triggered {
                event: EventPattern::PermanentEnters(land_you_control()),
                condition: None,
                intervening_if: false,
                effect: Effect::PutCounters {
                    what: EffectTarget::SourceSelf,
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(1),
                },
            }],
        )
        .with_text("Landfall — Whenever a land you control enters, put a +1/+1 counter on this creature."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn sazhs_chocobo_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SAZHS_CHOCOBO).unwrap();
        assert_eq!(def.chars.power, Some(0));
        assert_eq!(def.chars.toughness, Some(1));
        assert_eq!(def.chars.subtypes, vec![CreatureType::Bird.into()]);
        assert!(!def.is_mana_source());
        expect![[r#"
            [
                Triggered {
                    event: PermanentEnters(
                        All(
                            [
                                HasCardType(
                                    Land,
                                ),
                                ControlledBy(
                                    Controller,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: PutCounters {
                        what: SourceSelf,
                        kind: PlusOnePlusOne,
                        n: Fixed(
                            1,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: resolving the landfall trigger puts a +1/+1 counter on the 0/1 Chocobo → 1/2.
    #[test]
    fn sazhs_chocobo_landfall_adds_a_counter() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(SAZHS_CHOCOBO).unwrap().chars.clone();
        let chocobo = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let cc = state.computed(chocobo);
        assert_eq!((cc.power, cc.toughness), (Some(0), Some(1)));
        let landfall = match &state.card_db().get(SAZHS_CHOCOBO).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected landfall Triggered, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &landfall,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(chocobo), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let cc = e.state.computed(chocobo);
        assert_eq!((cc.power, cc.toughness), (Some(1), Some(2))); // a +1/+1 counter
    }

    /// #60 end-to-end (the REAL play-land + trigger loop): playing a land you control fires landfall
    /// — "Whenever a land you control enters, put a +1/+1 counter on this creature." Driven via
    /// `play_land` → `run_agenda` (stacks the trigger) → `resolve_top` (resolves it): the 0/1 Chocobo
    /// becomes a 1/2. A SECOND land fires it again → 2/3 (the trigger isn't one-shot).
    #[test]
    fn sazhs_chocobo_landfall_via_real_land_drop() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{CounterKind, Zone};
        use crate::cards::{grp, starter_db};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        #[derive(Clone)]
        struct PassiveAgent;
        impl Agent for PassiveAgent {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let chocobo = {
            let c = state.card_db().get(SAZHS_CHOCOBO).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let hand: Vec<_> = (0..2)
            .map(|_| {
                let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Hand)
            })
            .collect();
        let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);

        // Drive a real land drop and resolve every trigger it spawns to a stable state.
        let play_and_settle = |e: &mut Engine, land| {
            e.play_land(PlayerId(0), land);
            e.run_agenda();
            while !e.state.stack.items.is_empty() {
                e.resolve_top();
                e.run_agenda();
            }
        };

        play_and_settle(&mut e, hand[0]);
        assert_eq!(
            e.state.object(chocobo).counters.get(&CounterKind::PlusOnePlusOne),
            1,
            "first land → landfall → one +1/+1 counter (0/1 → 1/2)"
        );
        play_and_settle(&mut e, hand[1]);
        assert_eq!(
            e.state.object(chocobo).counters.get(&CounterKind::PlusOnePlusOne),
            2,
            "second land → landfall fires again → two counters (→ 2/3)"
        );
        let cc = e.state.computed(chocobo);
        assert_eq!((cc.power, cc.toughness), (Some(2), Some(3)));
    }
}
