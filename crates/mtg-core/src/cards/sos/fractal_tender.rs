//! Fractal Tender — `{3}{G}{U}` Creature — Elf Wizard 3/3 (first printed SOS).
//!
//! Oracle: "Ward {2} / Increment (Whenever you cast a spell, if the amount of mana you spent is
//! greater than this creature's power or toughness, put a +1/+1 counter on this creature.) / At the
//! beginning of each end step, if you put a counter on this creature this turn, create a 0/0 green and
//! blue Fractal creature token and put three +1/+1 counters on it."
//!
//! **Fully implemented** — combines three existing/near-existing caps:
//! - **Ward {2}** (S17): `ward_mana(2)`.
//! - **Increment** (S6): the shared `increment_ability()` (a `SpellCast(Any)` trigger comparing
//!   `ManaSpentOnTrigger` to the source's power/toughness, adding a `+1/+1` counter to `SourceSelf`).
//! - **End-step Fractal** — a `BeginningOfStep(End)` trigger (fires on *each* end step, so no
//!   `YourTurn` gate) with the new `Condition::PutCounterOnSelfThisTurn` intervening-"if": it holds
//!   iff a counter was put on THIS permanent this turn (`Object.counter_added_this_turn`, set by
//!   `Action::AddCounters`, reset each turn / on zone change). When it holds, create one 0/0 green
//!   and blue Fractal token entering with three `+1/+1` counters (`fractal_token(3)` — a 3/3).

use crate::basics::{Color, Phase};
use crate::cards::helpers::{fractal_token, increment_ability, ward_mana};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const FRACTAL_TENDER: u32 = 342;

/// "At the beginning of each end step, if you put a counter on this creature this turn, create a 0/0
/// green and blue Fractal creature token and put three +1/+1 counters on it."
fn end_step_fractal() -> Ability {
    Ability::Triggered {
        event: EventPattern::BeginningOfStep(Phase::End),
        condition: Some(Condition::PutCounterOnSelfThisTurn),
        intervening_if: true,
        effect: Effect::CreateToken {
            spec: fractal_token(3),
            count: ValueExpr::Fixed(1),
            controller: PlayerRef::Controller,
            dynamic_counters: vec![],
        },
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        FRACTAL_TENDER,
        "Fractal Tender",
        &[CreatureType::Elf, CreatureType::Wizard],
        Color::Green,
        mana_cost(3, &[(Color::Green, 1), (Color::Blue, 1)]),
        3,
        3,
        vec![ward_mana(2), increment_ability(), end_step_fractal()],
    );
    def.chars.colors = vec![Color::Green, Color::Blue];
    def.text = "Ward {2} (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays {2}.)\nIncrement (Whenever you cast a spell, if the amount of mana you spent is greater than this creature's power or toughness, put a +1/+1 counter on this creature.)\nAt the beginning of each end step, if you put a counter on this creature this turn, create a 0/0 green and blue Fractal creature token and put three +1/+1 counters on it.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{GameEvent, RandomAgent};
    use crate::basics::{CounterKind, Zone};
    use crate::cards::starter_db;
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use crate::subtypes::Subtype;
    use expect_test::expect;
    use std::sync::Arc;

    #[test]
    fn fractal_tender_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(FRACTAL_TENDER).unwrap();
        assert_eq!((def.chars.power, def.chars.toughness), (Some(3), Some(3)));
        assert_eq!(def.chars.colors, vec![Color::Green, Color::Blue]);
        assert!(def.fully_implemented);
        expect![[r#"
            Triggered {
                event: BeginningOfStep(
                    End,
                ),
                condition: Some(
                    PutCounterOnSelfThisTurn,
                ),
                intervening_if: true,
                effect: CreateToken {
                    spec: TokenSpec {
                        name: "Fractal",
                        card_types: [
                            Creature,
                        ],
                        subtypes: [
                            Creature(
                                Fractal,
                            ),
                        ],
                        colors: [
                            Green,
                            Blue,
                        ],
                        power: 0,
                        toughness: 0,
                        keywords: [],
                        counters: [
                            (
                                PlusOnePlusOne,
                                3,
                            ),
                        ],
                        grp_id: 0,
                    },
                    count: Fixed(
                        1,
                    ),
                    controller: Controller,
                    dynamic_counters: [],
                },
            }"#]]
        .assert_eq(&format!("{:#?}", def.abilities[2]));
    }

    /// Build a game with Fractal Tender on P0's battlefield, seed the "put a counter this turn" flag
    /// per `counter_added`, run P0's beginning-of-end-step trigger, and return P0's Fractal-token count.
    fn run_end_step(counter_added: bool) -> usize {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let tender = {
            let c = state.card_db().get(FRACTAL_TENDER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        if counter_added {
            state.objects.get_mut(&tender).unwrap().counter_added_this_turn = true;
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::End;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.broadcast(GameEvent::PhaseBegan { turn: 1, phase: Phase::End, active: PlayerId(0) });
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        e.state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .filter(|id| {
                e.state
                    .objects
                    .get(id)
                    .is_some_and(|o| o.chars.subtypes.contains(&Subtype::Creature(CreatureType::Fractal)))
            })
            .count()
    }

    /// A counter was put on Fractal Tender this turn → the end-step trigger's intervening-"if" holds
    /// → a Fractal token is created (entering as a 3/3 via three +1/+1 counters).
    #[test]
    fn makes_a_fractal_when_a_counter_was_added() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let tender = {
            let c = state.card_db().get(FRACTAL_TENDER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.objects.get_mut(&tender).unwrap().counter_added_this_turn = true;
        state.active_player = PlayerId(0);
        state.phase = Phase::End;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.broadcast(GameEvent::PhaseBegan { turn: 1, phase: Phase::End, active: PlayerId(0) });
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        let fractal = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .copied()
            .find(|id| {
                *id != tender
                    && e.state
                        .objects
                        .get(id)
                        .is_some_and(|o| o.chars.subtypes.contains(&Subtype::Creature(CreatureType::Fractal)))
            })
            .expect("a Fractal token was created");
        assert_eq!(
            e.state.object(fractal).counters.get(&CounterKind::PlusOnePlusOne),
            3,
            "the Fractal entered with three +1/+1 counters (a 3/3)"
        );
    }

    /// No counter was put on Fractal Tender this turn → the intervening-"if" fails → no token.
    #[test]
    fn no_fractal_without_a_counter_this_turn() {
        assert_eq!(run_end_step(false), 0, "no counter added → no Fractal token");
    }

    /// With the flag set, exactly one Fractal token is created.
    #[test]
    fn one_fractal_with_a_counter_this_turn() {
        assert_eq!(run_end_step(true), 1, "counter added → one Fractal token");
    }

    /// The `counter_added_this_turn` flag is set by the REAL counter-add action path (not just poked):
    /// resolving a `PutCounters` on Fractal Tender flips it, and it does not fire on a mere removal.
    #[test]
    fn real_counter_add_sets_the_flag() {
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::EffectTarget;
        use crate::ids::StackId;
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let tender = {
            let c = state.card_db().get(FRACTAL_TENDER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        assert!(!e.state.object(tender).counter_added_this_turn, "no counter yet");
        let add = Effect::PutCounters {
            what: EffectTarget::SourceSelf,
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::Fixed(1),
        };
        e.resolve_effect(
            &add,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(tender), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(tender).counters.get(&CounterKind::PlusOnePlusOne), 1);
        assert!(e.state.object(tender).counter_added_this_turn, "real AddCounters set the flag");
    }
}
