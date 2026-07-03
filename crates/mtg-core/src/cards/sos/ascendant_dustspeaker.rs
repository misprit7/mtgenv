//! Ascendant Dustspeaker — `{4}{W}` Creature — Orc Cleric 3/4 (first printed SOS).
//!
//! Oracle: "Flying / When this creature enters, put a +1/+1 counter on another target creature you
//! control. / At the beginning of combat on your turn, exile up to one target card from a graveyard."
//!
//! **Fully implemented** — printed Flying, plus two triggers:
//! 1. an ETB (`SelfEnters`) `PutCounters` on **another** target creature you control — the "another"
//!    is `Not(ItSelf)`, now honoured by targeting: `target_candidates` threads the ability's source
//!    into `target_matches_filter`, so the Dustspeaker excludes itself from its own candidate list.
//! 2. a begin-combat (your turn) `Exile` of "up to one" (`min: 0`) target card from any graveyard —
//!    the same begin-of-step trigger idiom as Startled Relic Sloth.

use crate::basics::{Color, CounterKind, Phase, Zone};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ASCENDANT_DUSTSPEAKER: u32 = 328;

/// "another target creature you control" — a creature you control that is not this source (CR 601.2c).
fn another_creature_you_control() -> TargetSpec {
    TargetSpec {
        kind: TargetKind::Creature(CardFilter::All(vec![
            CardFilter::ControlledBy(PlayerRef::Controller),
            CardFilter::Not(Box::new(CardFilter::ItSelf)),
        ])),
        min: 1,
        max: 1,
        distinct: true,
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        ASCENDANT_DUSTSPEAKER,
        "Ascendant Dustspeaker",
        &[CreatureType::Orc, CreatureType::Cleric],
        Color::White,
        mana_cost(4, &[(Color::White, 1)]),
        3,
        4,
        vec![
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::PutCounters {
                    what: EffectTarget::Target(another_creature_you_control()),
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(1),
                },
            },
            Ability::Triggered {
                event: EventPattern::BeginningOfStep(Phase::BeginCombat),
                condition: Some(Condition::YourTurn),
                intervening_if: false,
                effect: Effect::Exile {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::CardInZone { zone: Zone::Graveyard, filter: CardFilter::Any },
                        min: 0,
                        max: 1,
                        distinct: true,
                    }),
                },
            },
        ],
    );
    def.chars.keywords = vec![Keyword::Flying];
    def.text = "Flying\nWhen this creature enters, put a +1/+1 counter on another target creature you control.\nAt the beginning of combat on your turn, exile up to one target card from a graveyard.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn ascendant_dustspeaker_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ASCENDANT_DUSTSPEAKER).unwrap();
        assert_eq!(def.chars.power, Some(3));
        assert_eq!(def.chars.keywords, vec![Keyword::Flying]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: PutCounters {
                        what: Target(
                            TargetSpec {
                                kind: Creature(
                                    All(
                                        [
                                            ControlledBy(
                                                Controller,
                                            ),
                                            Not(
                                                ItSelf,
                                            ),
                                        ],
                                    ),
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        kind: PlusOnePlusOne,
                        n: Fixed(
                            1,
                        ),
                    },
                },
                Triggered {
                    event: BeginningOfStep(
                        BeginCombat,
                    ),
                    condition: Some(
                        YourTurn,
                    ),
                    intervening_if: false,
                    effect: Exile {
                        what: Target(
                            TargetSpec {
                                kind: CardInZone {
                                    zone: Graveyard,
                                    filter: Any,
                                },
                                min: 0,
                                max: 1,
                                distinct: true,
                            },
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// A shared `ChooseTargets` agent: picks the first legal candidate of each non-empty slot.
    #[derive(Clone)]
    struct PickTargetAgent;
    impl crate::agent::Agent for PickTargetAgent {
        fn decide(
            &mut self,
            _v: &crate::agent::PlayerView,
            req: &crate::agent::DecisionRequest,
        ) -> crate::agent::DecisionResponse {
            use crate::agent::{DecisionRequest, DecisionResponse};
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => DecisionResponse::Pairs(
                    slots
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| !s.legal.is_empty())
                        .map(|(i, _)| (i as u32, 0))
                        .collect(),
                ),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Fire the ETB through the real turn engine with `Dustspeaker` entering; returns the engine so
    /// callers can inspect where the +1/+1 counter landed.
    fn run_etb(with_other_creature: bool) -> (crate::priority::Engine, crate::ids::ObjId, Option<crate::ids::ObjId>) {
        use crate::agent::GameEvent;
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::ids::PlayerId;
        use crate::priority::Engine;

        let mut state = build_game(1, &[&[], &[]]);
        let dust = {
            let c = state.card_db().get(ASCENDANT_DUSTSPEAKER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let other = with_other_creature.then(|| {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        });
        let mut e = Engine::new(state, vec![Box::new(PickTargetAgent), Box::new(PickTargetAgent)]);
        // The ETB (SelfEnters) trigger fires on the enter event, choosing its target now.
        e.broadcast(GameEvent::ObjectMoved { obj: dust, to: Zone::Battlefield });
        e.run_agenda();
        if !e.state.stack.is_empty() {
            e.resolve_top();
        }
        (e, dust, other)
    }

    /// Behaviour: with a second creature present, the ETB's "another target" puts its +1/+1 counter on
    /// the OTHER creature — and never on the Dustspeaker itself (self-exclusion via `Not(ItSelf)`).
    #[test]
    fn etb_counter_lands_on_the_other_creature() {
        use crate::basics::CounterKind as CK;
        let (e, dust, other) = run_etb(true);
        let other = other.unwrap();
        assert_eq!(e.state.object(other).counters.get(&CK::PlusOnePlusOne), 1, "counter on the other creature");
        assert_eq!(e.state.object(dust).counters.get(&CK::PlusOnePlusOne), 0, "never on itself");
    }

    /// Self-exclusion end-to-end: when the Dustspeaker is the ONLY creature you control, its
    /// "another target creature you control" has no legal target, so the trigger is removed (CR
    /// 603.3c) and no counter is placed anywhere. Proves the source-threaded `Not(ItSelf)` at the
    /// targeting layer (not just at resolution).
    #[test]
    fn etb_with_no_other_creature_fizzles() {
        use crate::basics::CounterKind as CK;
        let (e, dust, _) = run_etb(false);
        assert_eq!(e.state.object(dust).counters.get(&CK::PlusOnePlusOne), 0, "no self-counter; trigger removed");
        assert!(e.state.stack.is_empty(), "trigger left the stack with no legal target");
    }
}
