//! Earthbender Ascension — `{2}{G}` Enchantment (first printed TLA, Avatar: The Last Airbender).
//!
//! Oracle:
//!   When this enchantment enters, earthbend 2. Then search your library for a basic land card, put
//!   it onto the battlefield tapped, then shuffle.
//!   Landfall — Whenever a land you control enters, put a quest counter on this enchantment. When you
//!   do, if it has four or more quest counters on it, put a +1/+1 counter on target creature you
//!   control. It gains trample until end of turn.
//!
//! **Fully implemented** — both abilities faithful:
//! - "When this enchantment enters, **earthbend 2**. Then search your library for a basic land card,
//!   put it onto the battlefield tapped, then shuffle." — a `Triggered{SelfEnters}` over
//!   `Sequence[ Earthbend{target: land you control, n: 2}, fetch_basic_tapped() ]` (C12 + C5).
//!   (Earthbend, incl. its "dies/exiled → return tapped" delayed trigger, fully landed in C12.)
//! - "Landfall — Whenever a land you control enters, put a quest counter on this enchantment. When you
//!   do, if it has four or more quest counters on it, put a +1/+1 counter on target creature you
//!   control. It gains trample until end of turn." — a `Triggered{PermanentEnters(land you control)}`
//!   over `Sequence[ PutCounters{SourceSelf, Named("quest"), 1}, Conditional{ ValueAtLeast(
//!   CountersOnSelf(Named("quest")), 4), then: [ +1/+1 on target creature you control, trample until
//!   EOT ] } ]`. The "When you do … if ≥4 … target creature" is a **reflexive sub-trigger** (CR 603.7c,
//!   cap 2e13694): the quest counter is put unconditionally, then *only if* ≥4 quest counters does the
//!   reflexive ability go on the stack and choose its target (so sub-4 landfalls never prompt a target,
//!   and the counter always lands even with no creatures). `GrantKeyword{ChosenIndex(0), Trample,
//!   UntilEndOfTurn}` reuses the +1/+1's chosen creature.

use crate::basics::{Color, CounterKind};
use crate::cards::helpers::{earthbend, fetch_basic_tapped, land_you_control};
use crate::cards::{enchantment, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const EARTHBENDER_ASCENSION: u32 = 114;

pub fn register(db: &mut CardDb) {
    let mut def = enchantment(
        EARTHBENDER_ASCENSION,
        "Earthbender Ascension",
        Color::Green,
        mana_cost(2, &[(Color::Green, 1)]),
        vec![
            // "When this enchantment enters, earthbend 2. Then search your library for a basic land
            // card, put it onto the battlefield tapped, then shuffle."
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::Sequence(vec![earthbend(2), fetch_basic_tapped()]),
            },
            // "Landfall — Whenever a land you control enters, put a quest counter on this enchantment.
            // When you do, if it has four or more quest counters on it, put a +1/+1 counter on target
            // creature you control. It gains trample until end of turn."
            Ability::Triggered {
                event: EventPattern::PermanentEnters(land_you_control()),
                condition: None,
                intervening_if: false,
                effect: Effect::Sequence(vec![
                    Effect::PutCounters {
                        what: EffectTarget::SourceSelf,
                        kind: CounterKind::Named("quest".to_string()),
                        n: ValueExpr::Fixed(1),
                    },
                    // "When you do, if ≥4 quest counters …" — a reflexive sub-trigger (CR 603.7c): the
                    // targeted reward is deferred, its target chosen only when the intervening-if holds.
                    Effect::Conditional {
                        cond: Condition::ValueAtLeast(
                            ValueExpr::CountersOnSelf(CounterKind::Named("quest".to_string())),
                            ValueExpr::Fixed(4),
                        ),
                        then: Box::new(Effect::Sequence(vec![
                            Effect::PutCounters {
                                what: EffectTarget::Target(TargetSpec {
                                    kind: TargetKind::Creature(CardFilter::ControlledBy(
                                        PlayerRef::Controller,
                                    )),
                                    min: 1,
                                    max: 1,
                                    distinct: true,
                                }),
                                kind: CounterKind::PlusOnePlusOne,
                                n: ValueExpr::Fixed(1),
                            },
                            // "It gains trample until end of turn." — the same chosen creature.
                            Effect::GrantKeyword {
                                what: EffectTarget::ChosenIndex(0),
                                keyword: Keyword::Trample,
                                duration: Duration::UntilEndOfTurn,
                            },
                        ])),
                        otherwise: None,
                    },
                ]),
            },
        ],
    );
    def.text = "When this enchantment enters, earthbend 2. Then search your library for a basic land card, put it onto the battlefield tapped, then shuffle.\nLandfall — Whenever a land you control enters, put a quest counter on this enchantment. When you do, if it has four or more quest counters on it, put a +1/+1 counter on target creature you control. It gains trample until end of turn.".to_string();
    // Fully implemented: ETB earthbend+fetch (C12+C5) + the landfall quest-chain with the reflexive
    // sub-trigger reward (cap 2e13694). See module docs.
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;
    use expect_test::expect;

    #[test]
    fn earthbender_ascension_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(EARTHBENDER_ASCENSION).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Enchantment]);
        // Fully implemented: ETB earthbend+fetch + the landfall quest-chain (reflexive reward).
        assert!(def.fully_implemented);
        // ETB earthbend-then-fetch + landfall → quest counter → reflexive Conditional(≥4) reward.
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Sequence(
                        [
                            Earthbend {
                                target: Target(
                                    TargetSpec {
                                        kind: Permanent(
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
                                        min: 1,
                                        max: 1,
                                        distinct: true,
                                    },
                                ),
                                n: Fixed(
                                    2,
                                ),
                            },
                            Search {
                                who: Controller,
                                zone: Library,
                                filter: All(
                                    [
                                        HasCardType(
                                            Land,
                                        ),
                                        Supertype(
                                            Basic,
                                        ),
                                    ],
                                ),
                                min: 0,
                                max: 1,
                                to: ZoneDest {
                                    zone: Battlefield,
                                    pos: Any,
                                },
                                tapped: true,
                            },
                        ],
                    ),
                },
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
                    effect: Sequence(
                        [
                            PutCounters {
                                what: SourceSelf,
                                kind: Named(
                                    "quest",
                                ),
                                n: Fixed(
                                    1,
                                ),
                            },
                            Conditional {
                                cond: ValueAtLeast(
                                    CountersOnSelf(
                                        Named(
                                            "quest",
                                        ),
                                    ),
                                    Fixed(
                                        4,
                                    ),
                                ),
                                then: Sequence(
                                    [
                                        PutCounters {
                                            what: Target(
                                                TargetSpec {
                                                    kind: Creature(
                                                        ControlledBy(
                                                            Controller,
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
                                        GrantKeyword {
                                            what: ChosenIndex(
                                                0,
                                            ),
                                            keyword: Trample,
                                            duration: UntilEndOfTurn,
                                        },
                                    ],
                                ),
                                otherwise: None,
                            },
                        ],
                    ),
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: the ETB ability earthbends a target land you control (→ a 2/2 land-creature) and
    /// then fetches a basic onto the battlefield tapped.
    #[test]
    fn earthbender_etb_animates_a_land_and_fetches() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{CardType, Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        #[derive(Clone)]
        struct TakeItAgent;
        impl Agent for TakeItAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                    DecisionRequest::SelectCards { from, min, max, .. } => {
                        let n = (*min).max(1).min(*max).min(from.len() as u32);
                        DecisionResponse::Indices((0..n).collect())
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = build_game(1, &[&[grp::FOREST], &[]]); // library = a Forest to fetch
        let asc_chars = state.card_db().get(EARTHBENDER_ASCENSION).unwrap().chars.clone();
        let forest_chars = state.card_db().get(grp::FOREST).unwrap().chars.clone();
        let asc = state.add_card(PlayerId(0), asc_chars, Zone::Battlefield);
        let land = state.add_card(PlayerId(0), forest_chars, Zone::Battlefield); // the land to earthbend
        let etb = match &state.card_db().get(EARTHBENDER_ASCENSION).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected SelfEnters Triggered, got {o:?}"),
        };
        let mut e = Engine::new(state, vec![Box::new(TakeItAgent), Box::new(TakeItAgent)]);
        e.resolve_effect(
            &etb,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                source: Some(asc),
                chosen_targets: vec![Target::Object(land)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        let cc = e.state.computed(land);
        assert!(cc.is_creature() && cc.card_types.contains(&CardType::Land)); // earthbent: land-creature
        assert_eq!((cc.power, cc.toughness), (Some(2), Some(2))); // 0/0 + two +1/+1 counters
        assert_eq!(e.state.players[0].library.len(), 0); // the basic was fetched out of the library
    }

    /// #60 end-to-end (REAL cast → ETB trigger): cast Earthbender Ascension `{2}{G}`; on entering, its
    /// "earthbend 2, then fetch a basic" trigger goes on the stack (prompting `ChooseTargets` for the
    /// land to earthbend) and resolves — the chosen land becomes a 2/2 land-creature and a basic is
    /// fetched onto the battlefield tapped. Drives `cast_spell` → `resolve_top` → `run_agenda` → drain.
    #[test]
    fn earthbender_etb_via_real_cast() {
        use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{CardType, Target, Zone};
        use crate::cards::{grp, starter_db};
        use crate::ids::{ObjId, PlayerId};
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        #[derive(Clone)]
        struct PlayAgent {
            land: ObjId,
        }
        impl Agent for PlayAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::ChooseTargets { slots, .. } => {
                        let i = slots[0]
                            .legal
                            .iter()
                            .position(|t| matches!(t, Target::Object(o) if *o == self.land))
                            .unwrap_or(0);
                        DecisionResponse::Pairs(vec![(0, i as u32)])
                    }
                    DecisionRequest::SelectCards { from, min, max, .. } => {
                        let n = (*min).max(1).min(*max).min(from.len() as u32);
                        DecisionResponse::Indices((0..n).collect())
                    }
                    DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let asc = {
            let c = state.card_db().get(EARTHBENDER_ASCENSION).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let _ = asc;
        for _ in 0..3 {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield); // pays {2}{G}
        }
        let target_land = {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield) // the land to earthbend
        };
        {
            let c = state.card_db().get(grp::PLAINS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library); // a basic to fetch
        }
        let mut e = Engine::new(
            state,
            vec![Box::new(PlayAgent { land: target_land }), Box::new(PlayAgent { land: target_land })],
        );

        e.cast_spell(PlayerId(0), e.state.players[0].hand[0], CastVariant::Normal);
        e.resolve_top(); // Earthbender enters
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }

        let cc = e.state.computed(target_land);
        assert!(cc.is_creature() && cc.card_types.contains(&CardType::Land), "earthbent land-creature");
        assert_eq!((cc.power, cc.toughness), (Some(2), Some(2)), "earthbend 2 → 2/2");
        // The fetched basic (a Plains) is on the battlefield tapped; the library is empty.
        assert!(e.state.players[0].library.is_empty(), "basic fetched out of the library");
        let plains: Vec<_> = e.state.players[0]
            .battlefield
            .iter()
            .filter(|&&o| e.state.object(o).chars.grp_id == grp::PLAINS)
            .copied()
            .collect();
        assert_eq!(plains.len(), 1, "exactly one basic fetched");
        assert!(e.state.object(plains[0]).status.tapped, "the fetched basic enters tapped");
    }

    /// #60 end-to-end (REAL land drop → landfall + reflexive reward): with Earthbender already holding
    /// 3 quest counters, playing a land fires landfall — "put a quest counter (→ 4); when you do, if it
    /// has 4+ quest counters, put a +1/+1 counter on target creature you control and it gains trample."
    /// Drives `play_land` → `run_agenda` (stacks the trigger, prompting the reflexive target) →
    /// `resolve_top`: the 4th quest counter lands AND the chosen 2/2 becomes a 3/3 with trample.
    #[test]
    fn earthbender_landfall_quest_reward_via_real_land_drop() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{CounterKind, Target, Zone};
        use crate::cards::{grp, starter_db};
        use crate::effects::ability::Keyword;
        use crate::ids::{ObjId, PlayerId};
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        #[derive(Clone)]
        struct TargetAgent {
            want: ObjId,
        }
        impl Agent for TargetAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::ChooseTargets { slots, .. } => {
                        let i = slots[0]
                            .legal
                            .iter()
                            .position(|t| matches!(t, Target::Object(o) if *o == self.want))
                            .unwrap_or(0);
                        DecisionResponse::Pairs(vec![(0, i as u32)])
                    }
                    DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let asc = {
            let c = state.card_db().get(EARTHBENDER_ASCENSION).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(); // 2/2
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let land = {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        // Pre-seed 3 quest counters so this land drop pushes it to the 4+ reward threshold.
        let quest = CounterKind::Named("quest".into());
        state.objects.get_mut(&asc).unwrap().counters.counts.insert(quest.clone(), 3);

        let mut e = Engine::new(
            state,
            vec![Box::new(TargetAgent { want: bears }), Box::new(TargetAgent { want: bears })],
        );
        e.play_land(PlayerId(0), land);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }

        assert_eq!(e.state.object(asc).counters.get(&quest), 4, "landfall added the 4th quest counter");
        let cc = e.state.computed(bears);
        assert_eq!((cc.power, cc.toughness), (Some(3), Some(3)), "≥4 quest → +1/+1 on the target (2/2 → 3/3)");
        assert!(cc.has_keyword(Keyword::Trample), "and it gains trample until end of turn");
    }
}
