//! Berta, Wise Extrapolator — `{2}{G}{U}` Legendary Creature — Frog Druid 1/4 (first printed SOS).
//!
//! Oracle:
//! - "Increment (Whenever you cast a spell, if the amount of mana you spent is greater than this
//!   creature's power or toughness, put a +1/+1 counter on this creature.)"
//! - "Whenever one or more +1/+1 counters are put on Berta, add one mana of any color."
//! - "{X}, {T}: Create a 0/0 green and blue Fractal creature token and put X +1/+1 counters on it."
//!
//! **Fully implemented.** Increment = the shared `increment_ability()` (S6). The counters-put-on-self
//! trigger reuses `EventPattern::CountersPutOnSelf { +1/+1 }` (fired by the `AddCounters` executor)
//! with an `AddMana` effect adding one mana of any color (`ManaSpec.any_color`). The `{X}, {T}`
//! activated ability exercises the new **{X}-in-an-activated-cost** cap: `activate_ability` chooses X
//! (bounded by affordable mana), folds it into the mana paid, and carries it on the stack object so
//! `ValueExpr::X` reads it at resolution; the effect is the shared 0/0 Fractal (`fractal_token(0)`)
//! entering with X +1/+1 counters via `CreateToken.dynamic_counters` — so it enters as an X/X.

use crate::basics::{Color, CounterKind};
use crate::cards::helpers::{fractal_token, increment_ability};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, EventPattern, Timing};
use crate::effects::target::ManaSpec;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const BERTA_WISE_EXTRAPOLATOR: u32 = 350;

/// "Whenever one or more +1/+1 counters are put on Berta, add one mana of any color."
fn mana_on_counter() -> Ability {
    Ability::Triggered {
        event: EventPattern::CountersPutOnSelf { kind: CounterKind::PlusOnePlusOne },
        condition: None,
        intervening_if: false,
        effect: Effect::AddMana {
            who: PlayerRef::Controller,
            mana: ManaSpec {
                produces: Vec::new(),
                any_color: Some(ValueExpr::Fixed(1)),
                restriction: None,
            },
        },
    }
}

/// "{X}, {T}: Create a 0/0 green and blue Fractal creature token and put X +1/+1 counters on it."
fn make_fractal() -> Ability {
    let mut mana = mana_cost(0, &[]);
    mana.x = 1; // one `{X}` in the activation cost (CR 107.3).
    Ability::Activated {
        cost: Cost { mana: Some(mana), components: vec![CostComponent::TapSelf] },
        effect: Effect::CreateToken {
            spec: fractal_token(0),
            count: ValueExpr::Fixed(1),
            controller: PlayerRef::Controller,
            dynamic_counters: vec![(CounterKind::PlusOnePlusOne, ValueExpr::X)],
        },
        timing: Timing::Instant,
        restriction: None,
        is_mana: false,
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        BERTA_WISE_EXTRAPOLATOR,
        "Berta, Wise Extrapolator",
        &[CreatureType::Frog, CreatureType::Druid],
        Color::Green,
        mana_cost(2, &[(Color::Green, 1), (Color::Blue, 1)]),
        1,
        4,
        vec![increment_ability(), mana_on_counter(), make_fractal()],
    );
    def.chars.colors = vec![Color::Green, Color::Blue];
    def.chars.supertypes = vec![Supertype::Legendary];
    def.text = "Increment (Whenever you cast a spell, if the amount of mana you spent is greater than this creature's power or toughness, put a +1/+1 counter on this creature.)\nWhenever one or more +1/+1 counters are put on Berta, add one mana of any color.\n{X}, {T}: Create a 0/0 green and blue Fractal creature token and put X +1/+1 counters on it.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{
        AbilityRef, Agent, DecisionRequest, DecisionResponse, NumberReason, PlayerView, RandomAgent,
    };
    use crate::basics::{CardType, Zone};
    use crate::cards::starter_db;
    use crate::effects::ability::Ability as Ab;
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use expect_test::expect;
    use std::sync::Arc;

    #[test]
    fn berta_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BERTA_WISE_EXTRAPOLATOR).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Green, Color::Blue]);
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert!(def.fully_implemented);
        // The activation cost carries one `{X}` pip and taps the source.
        let make = match &def.abilities[2] {
            Ab::Activated { cost, .. } => cost.clone(),
            o => panic!("expected the Fractal activated ability, got {o:?}"),
        };
        assert_eq!(make.mana.as_ref().unwrap().x, 1, "one {{X}} in the activation cost");
        expect![[r#"
            Activated {
                cost: Cost {
                    mana: Some(
                        ManaCost {
                            generic: 0,
                            colored: {},
                            x: 1,
                            hybrid: [],
                            mono_hybrid: [],
                            phyrexian: [],
                        },
                    ),
                    components: [
                        TapSelf,
                    ],
                },
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
                        counters: [],
                        grp_id: 0,
                    },
                    count: Fixed(
                        1,
                    ),
                    controller: Controller,
                    dynamic_counters: [
                        (
                            PlusOnePlusOne,
                            X,
                        ),
                    ],
                },
                timing: Instant,
                restriction: None,
                is_mana: false,
            }"#]]
        .assert_eq(&format!("{:#?}", def.abilities[2]));
    }

    /// An agent that always picks X = `self.0` for a `ChooseX`, else defers to a RandomAgent — so a
    /// `{X}` activation resolves with a known X through the real legality → pay → resolve loop.
    struct XAgent(i64, RandomAgent);
    impl Agent for XAgent {
        fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseNumber { reason: NumberReason::ChooseX, .. } => {
                    DecisionResponse::Number(self.0)
                }
                _ => self.1.decide(view, req),
            }
        }
    }

    /// Real-path integration: with Berta untapped and five Forests available, activating `{X}, {T}`
    /// with X=3 taps Berta, pays three generic, and resolves to a 0/0 Fractal that entered with three
    /// +1/+1 counters (a 3/3) — proving the chosen X is threaded through activation into `ValueExpr::X`.
    #[test]
    fn activating_x_makes_an_x_x_fractal() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let berta = {
            let c = state.card_db().get(BERTA_WISE_EXTRAPOLATOR).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // Five Forests to fund X=3 (plus Berta's own {T} is the ability's tap, not mana).
        for _ in 0..5 {
            let c = state.card_db().get(crate::cards::grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        let mut e = Engine::new(
            state,
            vec![Box::new(XAgent(3, RandomAgent::new(0))), Box::new(RandomAgent::new(1))],
        );
        let ability = AbilityRef(2);
        let bf_before = e.state.player(PlayerId(0)).battlefield.len();
        e.activate_ability(PlayerId(0), berta, ability);
        // Berta tapped for the {T}; three Forests tapped for the {3}.
        assert!(e.state.object(berta).status.tapped, "Berta tapped for the ability's {{T}}");
        let forests_tapped = e.state.player(PlayerId(0)).battlefield.iter().filter(|&&id| {
            e.state.object(id).chars.has_type(CardType::Land) && e.state.object(id).status.tapped
        }).count();
        assert_eq!(forests_tapped, 3, "three Forests tapped to pay {{X}}=3");
        // Resolve the ability off the stack.
        assert_eq!(e.state.stack.items.len(), 1, "ability is on the stack");
        e.resolve_top();
        e.run_agenda();
        let bf = &e.state.player(PlayerId(0)).battlefield;
        assert_eq!(bf.len(), bf_before + 1, "one Fractal token created");
        let token = *bf.last().unwrap();
        assert_eq!(
            e.state.object(token).counters.get(&CounterKind::PlusOnePlusOne),
            3,
            "entered with X=3 +1/+1 counters (a 3/3 Fractal)"
        );
        let cc = e.state.computed(token);
        assert_eq!((cc.power, cc.toughness), (Some(3), Some(3)), "computed as a 3/3");
    }

    /// Real-path: putting a +1/+1 counter on Berta fires her `CountersPutOnSelf` trigger, which
    /// resolves an `AddMana` of any color — leaving one mana floating in the controller's pool.
    #[test]
    fn counter_on_berta_adds_a_mana() {
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::EffectTarget;
        use crate::ids::StackId;
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let berta = {
            let c = state.card_db().get(BERTA_WISE_EXTRAPOLATOR).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.active_player = PlayerId(0);
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let pool_before: u32 = e.state.player(PlayerId(0)).mana_pool.amounts.values().sum();
        e.resolve_effect(
            &Effect::PutCounters {
                what: EffectTarget::SourceSelf,
                kind: CounterKind::PlusOnePlusOne,
                n: ValueExpr::Fixed(1),
            },
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(berta), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        let pool_after: u32 = e.state.player(PlayerId(0)).mana_pool.amounts.values().sum();
        assert_eq!(pool_after, pool_before + 1, "the counter trigger added one mana of any color");
    }
}
