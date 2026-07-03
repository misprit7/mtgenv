//! Dyadrine, Synthesis Amalgam — `{X}{G}{W}` Legendary Artifact Creature — Construct 0/1 (first
//! printed EOE, Edge of Eternities).
//!
//! Oracle:
//!   Trample
//!   Dyadrine enters with a number of +1/+1 counters on it equal to the amount of mana spent to cast
//!   it.
//!   Whenever you attack, you may remove a +1/+1 counter from each of two creatures you control. If
//!   you do, draw a card and create a 2/2 colorless Robot artifact creature token.
//!
//! **Fully implemented:**
//! - **Trample** (CR 702.19) — printed `Keyword`.
//! - **"Enters with +1/+1 counters equal to the mana spent to cast it"** — a `WouldEnterBattlefield(
//!   ItSelf)` replacement → `Rewrite::EntersWithCountersValue { PlusOnePlusOne, n: ValueExpr::ManaSpent }`
//!   (engine cap a2e2b13). `ManaSpent` = total mana paid at cast (generic + colored + the chosen X),
//!   reset on any zone change (CR 400.7), so Dyadrine cast for {3}{G}{W} (X=3) enters as a 5/6.
//! - **"Whenever you attack, you may remove a +1/+1 counter from each of two creatures you control. If
//!   you do, draw a card and create a 2/2 colorless Robot artifact creature token."** — a
//!   `Triggered{YouAttack}` over `IfYouDo{ cost: Optional{ ForEach{ select 2 creatures you control
//!   with a +1/+1 counter, body: PutCounters{Each, -1} } }, reward: Sequence[Draw, CreateToken{Robot}] }`
//!   (cap 0e01d56; "if you do" gate added #65). It's a *resolution-time choice* (no "target"). The
//!   reward is withheld unless the cost is **actually performed**: `IfYouDo` only runs the reward when
//!   its cost reports done, and the `ForEach{min:2}` reports done only when it removes a counter from
//!   each of *two* creatures — so declining the `Optional`, or controlling fewer than two
//!   counter-bearing creatures, yields no draw and no token (CR 603.7 reflexive "if you do" without a
//!   separate sub-trigger). Earlier this was `Optional{ Sequence[ForEach, Draw, CreateToken] }`, whose
//!   `Sequence` ran the draw + token even when the `ForEach` removed nothing (#65).

use crate::basics::{CardType, Color, CounterKind, Zone};
use crate::cards::{mana_cost, CardDb, CardDef};
use crate::effects::ability::{Ability, ActionPattern, EventPattern, Keyword, Rewrite};
use crate::effects::target::{CardFilter, SelectSpec, TokenSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::state::Characteristics;
use crate::subtypes::{CreatureType, Subtype, Supertype};

/// grp id (per-set ids live near their cards).
pub const DYADRINE_SYNTHESIS_AMALGAM: u32 = 116;

pub fn register(db: &mut CardDb) {
    // {X}{G}{W}: generic 0, one G + one W pip, one {X} symbol.
    let mut cost = mana_cost(0, &[(Color::Green, 1), (Color::White, 1)]);
    cost.x = 1;
    let chars = Characteristics {
        name: "Dyadrine, Synthesis Amalgam".to_string(),
        card_types: vec![CardType::Artifact, CardType::Creature],
        subtypes: vec![CreatureType::Construct.into()],
        supertypes: vec![Supertype::Legendary],
        colors: vec![Color::Green, Color::White],
        mana_cost: Some(cost),
        power: Some(0),
        toughness: Some(1),
        keywords: vec![Keyword::Trample],
        grp_id: DYADRINE_SYNTHESIS_AMALGAM,
        ..Default::default()
    };
    db.insert(CardDef {
        chars,
        abilities: vec![
            // "Dyadrine enters with a number of +1/+1 counters on it equal to the mana spent to cast it."
            Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersWithCountersValue {
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::ManaSpent,
                },
            },
            // "Whenever you attack, you may remove a +1/+1 counter from each of two creatures you
            // control. If you do, draw a card and create a 2/2 colorless Robot artifact creature token."
            Ability::Triggered {
                event: EventPattern::YouAttack,
                condition: None,
                intervening_if: false,
                // "you may [remove a +1/+1 counter from each of two creatures you control]. If you do,
                // [draw a card and create a Robot]." The reward is gated on the cost being *actually
                // performed* via `IfYouDo`: the `Optional` is the "may", and the `ForEach{min:2}` only
                // reports done if it removes a counter from each of two creatures — so declining, or
                // not controlling two counter-bearing creatures, yields no draw and no token (CR
                // reflexive "if you do"). The chars-only condition system can't see +1/+1 counters,
                // so this gate must ride the ForEach itself rather than a parallel `CountAtLeast`.
                effect: Effect::IfYouDo {
                    cost: Box::new(Effect::Optional {
                        prompt: "Remove a +1/+1 counter from each of two creatures you control?".to_string(),
                        // remove a +1/+1 counter from each of two chosen creatures you control
                        body: Box::new(Effect::ForEach {
                            selector: SelectSpec {
                                zone: Zone::Battlefield,
                                filter: CardFilter::All(vec![
                                    CardFilter::HasCardType(CardType::Creature),
                                    CardFilter::ControlledBy(PlayerRef::Controller),
                                    CardFilter::HasCounter(CounterKind::PlusOnePlusOne),
                                ]),
                                chooser: PlayerRef::Controller,
                                min: ValueExpr::Fixed(2),
                                max: ValueExpr::Fixed(2),
                            },
                            body: Box::new(Effect::PutCounters {
                                what: EffectTarget::Each,
                                kind: CounterKind::PlusOnePlusOne,
                                n: ValueExpr::Fixed(-1),
                            }),
                        }),
                    }),
                    // "If you do, draw a card and create a 2/2 colorless Robot artifact creature token."
                    reward: Box::new(Effect::Sequence(vec![
                        Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
                        Effect::CreateToken {
                            spec: TokenSpec {
                                grp_id: 0,
                                name: "Robot".to_string(),
                                card_types: vec![CardType::Artifact, CardType::Creature],
                                subtypes: vec![Subtype::Creature(CreatureType::Robot)],
                                colors: vec![],
                                power: 2,
                                toughness: 2,
                                keywords: vec![],
                                counters: vec![],
                            },
                            count: ValueExpr::Fixed(1),
                            controller: PlayerRef::Controller,
                        },
                    ])),
                },
            },
        ],
        text: "Trample\nDyadrine enters with a number of +1/+1 counters on it equal to the amount of mana spent to cast it.\nWhenever you attack, you may remove a +1/+1 counter from each of two creatures you control. If you do, draw a card and create a 2/2 colorless Robot artifact creature token.".to_string(),
        // Fully implemented: trample + enters-with-counters (ManaSpent) + the attack ability (cap 0e01d56).
        fully_implemented: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subtypes::Subtype;
    use expect_test::expect;

    #[test]
    fn dyadrine_synthesis_amalgam_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(DYADRINE_SYNTHESIS_AMALGAM).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Artifact, CardType::Creature]);
        assert_eq!(def.chars.subtypes, vec![Subtype::Creature(CreatureType::Construct)]);
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!(def.chars.colors, vec![Color::Green, Color::White]);
        assert_eq!(def.chars.keywords, vec![Keyword::Trample]); // trample works today
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().x, 1); // {X} symbol present
        assert_eq!((def.chars.power, def.chars.toughness), (Some(0), Some(1))); // base; counters add
        // Fully implemented: trample + enters-with-counters(ManaSpent) + the YouAttack ability.
        assert!(def.fully_implemented);
        // enters-with-counters-=-mana-spent replacement + the "whenever you attack" Optional/ForEach ability.
        expect![[r#"
            [
                Replacement {
                    pattern: WouldEnterBattlefield(
                        ItSelf,
                    ),
                    rewrite: EntersWithCountersValue {
                        kind: PlusOnePlusOne,
                        n: ManaSpent,
                    },
                },
                Triggered {
                    event: YouAttack,
                    condition: None,
                    intervening_if: false,
                    effect: IfYouDo {
                        cost: Optional {
                            prompt: "Remove a +1/+1 counter from each of two creatures you control?",
                            body: ForEach {
                                selector: SelectSpec {
                                    zone: Battlefield,
                                    filter: All(
                                        [
                                            HasCardType(
                                                Creature,
                                            ),
                                            ControlledBy(
                                                Controller,
                                            ),
                                            HasCounter(
                                                PlusOnePlusOne,
                                            ),
                                        ],
                                    ),
                                    chooser: Controller,
                                    min: Fixed(
                                        2,
                                    ),
                                    max: Fixed(
                                        2,
                                    ),
                                },
                                body: PutCounters {
                                    what: Each,
                                    kind: PlusOnePlusOne,
                                    n: Fixed(
                                        -1,
                                    ),
                                },
                            },
                        },
                        reward: Sequence(
                            [
                                Draw {
                                    who: Controller,
                                    count: Fixed(
                                        1,
                                    ),
                                },
                                CreateToken {
                                    spec: TokenSpec {
                                        name: "Robot",
                                        card_types: [
                                            Artifact,
                                            Creature,
                                        ],
                                        subtypes: [
                                            Creature(
                                                Robot,
                                            ),
                                        ],
                                        colors: [],
                                        power: 2,
                                        toughness: 2,
                                        keywords: [],
                                        counters: [],
                                        grp_id: 0,
                                    },
                                    count: Fixed(
                                        1,
                                    ),
                                    controller: Controller,
                                },
                            ],
                        ),
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: resolving the attack ability (you choose to) removes a +1/+1 counter from each of two
    /// creatures you control, then draws a card and makes a 2/2 Robot token. Snapshot of the resolved
    /// state (counters removed → both back to 2/2, hand +1, a Robot token on the battlefield).
    #[test]
    fn dyadrine_attack_removes_counters_draws_and_makes_a_robot() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        use expect_test::expect;

        // An agent that takes the optional ("yes") and picks the first `min` candidates to select.
        #[derive(Clone)]
        struct YesAgent;
        impl Agent for YesAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                    DecisionRequest::SelectCards { min, .. } => {
                        DecisionResponse::Indices((0..*min).collect())
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        // P0 has a 1-card library (so the draw resolves) + two 2/2 Grizzly Bears, each with a +1/+1
        // counter (a 3/3), + Dyadrine itself (the source).
        let mut state = build_game(1, &[&[grp::FOREST], &[]]);
        let bears_chars = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let dyadrine_chars = state.card_db().get(DYADRINE_SYNTHESIS_AMALGAM).unwrap().chars.clone();
        let dyadrine = state.add_card(PlayerId(0), dyadrine_chars, Zone::Battlefield);
        let bears1 = state.add_card(PlayerId(0), bears_chars.clone(), Zone::Battlefield);
        let bears2 = state.add_card(PlayerId(0), bears_chars, Zone::Battlefield);
        let attack = match &state.card_db().get(DYADRINE_SYNTHESIS_AMALGAM).unwrap().abilities[1] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected YouAttack Triggered, got {o:?}"),
        };
        let mut e = Engine::new(state, vec![Box::new(YesAgent), Box::new(YesAgent)]);
        let pp = CounterKind::PlusOnePlusOne;
        // Give each Bears a +1/+1 counter (a 3/3) so both are valid Select candidates.
        for b in [bears1, bears2] {
            e.resolve_effect(
                &Effect::PutCounters { what: EffectTarget::SourceSelf, kind: pp.clone(), n: ValueExpr::Fixed(1) },
                &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(b), ..Default::default() },
                WbReason::Resolve(StackId(0)),
            );
        }
        // Resolve Dyadrine's attack ability.
        e.resolve_effect(
            &attack,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(dyadrine), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        // Render the resolved state: bears' P/T (counters removed), hand size, and the new Robot token.
        let cc1 = e.state.computed(bears1);
        let cc2 = e.state.computed(bears2);
        let bf = &e.state.players[0].battlefield;
        let token = bf.iter().find(|id| ![dyadrine, bears1, bears2].contains(id)).copied();
        let token_pt = token.map(|t| {
            let c = e.state.computed(t);
            (c.power, c.toughness)
        });
        let render = format!(
            "bears1={:?} bears2={:?} | hand={} | battlefield={} | robot_token_pt={:?}",
            (cc1.power, cc1.toughness),
            (cc2.power, cc2.toughness),
            e.state.players[0].hand.len(),
            bf.len(),
            token_pt,
        );
        expect!["bears1=(Some(2), Some(2)) bears2=(Some(2), Some(2)) | hand=1 | battlefield=4 | robot_token_pt=Some((Some(2), Some(2)))"]
        .assert_eq(&render);
    }

    /// #60 end-to-end (the REAL cast path — the clause `resolve_effect` structurally can't test):
    /// "enters with +1/+1 counters equal to the **mana spent to cast it**". Cast Dyadrine `{X}{G}{W}`
    /// with X=3 via `cast_spell`, which asks `ChooseNumber{ChooseX}`, then auto-pays `{3}{G}{W}` (5
    /// mana) from five lands. The `EntersWithCountersValue{ManaSpent}` replacement applies at
    /// `resolve_top`'s commit → 5 counters → a 0/1 base becomes a **5/6**. This is the deck's most
    /// mana-dependent clause; driving it through real payment is the whole point of #60.
    #[test]
    fn dyadrine_enters_with_counters_equal_to_mana_spent_via_cast() {
        use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, NumberReason, PlayerView};
        use crate::cards::{grp, starter_db};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        // Chooses X=3; otherwise cooperative.
        #[derive(Clone)]
        struct XAgent;
        impl Agent for XAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::ChooseNumber { reason: NumberReason::ChooseX, .. } => {
                        DecisionResponse::Number(3)
                    }
                    DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let dyadrine = {
            let c = state.card_db().get(DYADRINE_SYNTHESIS_AMALGAM).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        // Five lands to pay {3}{G}{W} for X=3: 3 Forest (G + generic) + 2 Plains (W + generic).
        for _ in 0..3 {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        for _ in 0..2 {
            let c = state.card_db().get(grp::PLAINS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let mut e = Engine::new(state, vec![Box::new(XAgent), Box::new(XAgent)]);
        e.cast_spell(PlayerId(0), dyadrine, CastVariant::Normal); // ChooseX=3, auto-pays {3}{G}{W}
        e.resolve_top(); // enters → EntersWithCountersValue{ManaSpent} replacement applies

        assert_eq!(
            e.state.object(dyadrine).counters.get(&CounterKind::PlusOnePlusOne),
            5,
            "entered with +1/+1 counters equal to the 5 mana spent ({{3}}{{G}}{{W}}, X=3)"
        );
        let cc = e.state.computed(dyadrine);
        assert_eq!(cc.power, Some(5), "0/1 base + 5 counters = 5 power");
        assert_eq!(cc.toughness, Some(6), "0/1 base + 5 counters = 6 toughness");
        assert!(cc.has_keyword(crate::effects::ability::Keyword::Trample), "Trample");
    }

    /// #60 end-to-end (REAL attack declaration → trigger): "Whenever you attack, you may remove a
    /// +1/+1 counter from each of two creatures you control. If you do, draw a card and create a 2/2
    /// colorless Robot artifact creature token." Driven via `declare_attackers_explicit` (fires the
    /// YouAttack trigger) → `run_agenda` → `resolve_top`: with two 3/3 (counter-bearing) creatures, the
    /// reward fires — each drops to 2/2, the controller draws, and a 2/2 Robot token appears.
    #[test]
    fn dyadrine_attack_trigger_via_real_combat() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::cards::{grp, starter_db};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        #[derive(Clone)]
        struct YesAgent;
        impl Agent for YesAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                    DecisionRequest::SelectCards { min, .. } => DecisionResponse::Indices((0..*min).collect()),
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let dyadrine = {
            let c = state.card_db().get(DYADRINE_SYNTHESIS_AMALGAM).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let bears: Vec<_> = (0..2)
            .map(|_| {
                let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(); // 2/2
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            })
            .collect();
        {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library); // the card the trigger draws
        }
        state.active_player = PlayerId(0);
        // Each Bears holds a +1/+1 counter (a 3/3) so both are valid "remove a counter" targets.
        for &b in &bears {
            state.objects.get_mut(&b).unwrap().counters.counts.insert(CounterKind::PlusOnePlusOne, 1);
        }
        let bf_before = 3usize; // dyadrine + 2 bears
        let mut e = Engine::new(state, vec![Box::new(YesAgent), Box::new(YesAgent)]);

        e.declare_attackers_explicit(&[dyadrine]); // fires "whenever you attack"
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }

        // Each chosen creature lost its +1/+1 counter (3/3 → 2/2).
        for &b in &bears {
            assert_eq!(e.state.object(b).counters.get(&CounterKind::PlusOnePlusOne), 0, "a counter removed");
        }
        assert_eq!(e.state.players[0].hand.len(), 1, "drew a card off the reward");
        // A new 2/2 Robot token joined the battlefield.
        let token = e.state.players[0]
            .battlefield
            .iter()
            .find(|&&o| o != dyadrine && !bears.contains(&o))
            .copied();
        let token = token.expect("a Robot token was created");
        let tc = e.state.computed(token);
        assert_eq!((tc.power, tc.toughness), (Some(2), Some(2)), "2/2 Robot token");
        assert_eq!(e.state.players[0].battlefield.len(), bf_before + 1, "exactly one token added");
    }

    /// #65 regression: the reward is gated on the cost ("if you do"). With only **one** creature
    /// carrying a +1/+1 counter, "remove a +1/+1 counter from each of *two* creatures you control"
    /// can't be performed — so even when the controller says **yes** to the optional, NO counter is
    /// removed, NO card is drawn, and NO Robot token is created. (Before #65 the draw + token fired
    /// unconditionally because the reward sat in the same `Sequence` as the un-met `ForEach`.)
    #[test]
    fn dyadrine_attack_no_reward_when_fewer_than_two_counters() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::cards::{grp, starter_db};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        // Eagerly says "yes" to the optional and picks the first candidates — i.e. the worst case for
        // the gate: a cooperative controller who *wants* the reward but can't pay for it.
        #[derive(Clone)]
        struct YesAgent;
        impl Agent for YesAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                    DecisionRequest::SelectCards { min, .. } => DecisionResponse::Indices((0..*min).collect()),
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let dyadrine = {
            let c = state.card_db().get(DYADRINE_SYNTHESIS_AMALGAM).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let bears: Vec<_> = (0..2)
            .map(|_| {
                let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(); // 2/2
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            })
            .collect();
        {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library); // would-be drawn card; must stay put
        }
        state.active_player = PlayerId(0);
        // Only ONE creature has a +1/+1 counter — fewer than the two the cost requires (Dyadrine
        // itself entered without counters in this hand-built state, so it isn't a candidate either).
        state.objects.get_mut(&bears[0]).unwrap().counters.counts.insert(CounterKind::PlusOnePlusOne, 1);
        let bf_before = state.players[0].battlefield.len(); // dyadrine + 2 bears
        let mut e = Engine::new(state, vec![Box::new(YesAgent), Box::new(YesAgent)]);

        e.declare_attackers_explicit(&[dyadrine]); // fires "whenever you attack"
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }

        // Cost could not be paid → the single counter is untouched (all-or-nothing).
        assert_eq!(
            e.state.object(bears[0]).counters.get(&CounterKind::PlusOnePlusOne),
            1,
            "the lone counter is NOT removed when two can't be"
        );
        // Reward withheld: no draw, no token.
        assert_eq!(e.state.players[0].hand.len(), 0, "no draw without paying the cost");
        assert_eq!(
            e.state.players[0].battlefield.len(),
            bf_before,
            "no Robot token without paying the cost"
        );
    }
}
