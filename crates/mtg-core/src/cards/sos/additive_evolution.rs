//! Additive Evolution — `{3}{G}{G}` Enchantment (first printed SOS).
//!
//! Oracle: "When this enchantment enters, create a 0/0 green and blue Fractal creature token. Put
//! three +1/+1 counters on it. / At the beginning of combat on your turn, put a +1/+1 counter on
//! target creature you control. It gains vigilance until end of turn."
//!
//! **Fully implemented** — an ETB that makes the shared Fractal token entering with three +1/+1
//! counters (a 3/3, via `TokenSpec.counters`), plus a begin-combat trigger (your turn only) that
//! puts a +1/+1 counter on a target creature you control and grants it vigilance until end of turn.

use crate::basics::{Color, CounterKind, Phase};
use crate::cards::helpers::fractal_token;
use crate::cards::{enchantment, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const ADDITIVE_EVOLUTION: u32 = 219;

pub fn register(db: &mut CardDb) {
    let abilities = vec![
        Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            effect: Effect::CreateToken {
                spec: fractal_token(3),
                count: ValueExpr::Fixed(1),
                controller: PlayerRef::Controller,
            },
        },
        Ability::Triggered {
            event: EventPattern::BeginningOfStep(Phase::BeginCombat),
            condition: Some(Condition::YourTurn),
            intervening_if: false,
            effect: Effect::Sequence(vec![
                Effect::PutCounters {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(1),
                },
                Effect::GrantKeyword {
                    what: EffectTarget::ChosenIndex(0),
                    keyword: Keyword::Vigilance,
                    duration: Duration::UntilEndOfTurn,
                },
            ]),
        },
    ];
    db.insert(
        enchantment(ADDITIVE_EVOLUTION, "Additive Evolution", Color::Green, mana_cost(3, &[(Color::Green, 2)]), abilities)
            .with_text("When this enchantment enters, create a 0/0 green and blue Fractal creature token. Put three +1/+1 counters on it.\nAt the beginning of combat on your turn, put a +1/+1 counter on target creature you control. It gains vigilance until end of turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn additive_evolution_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ADDITIVE_EVOLUTION).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
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
                        },
                        count: Fixed(
                            1,
                        ),
                        controller: Controller,
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
                    effect: Sequence(
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
                                keyword: Vigilance,
                                duration: UntilEndOfTurn,
                            },
                        ],
                    ),
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: the ETB makes a 3/3 Fractal (0/0 entering with three +1/+1 counters).
    #[test]
    fn additive_evolution_makes_a_3_3_fractal() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(ADDITIVE_EVOLUTION).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let etb = match &state.card_db().get(ADDITIVE_EVOLUTION).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected ETB Triggered, got {o:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &etb,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let fractal = e.state.players[0]
            .battlefield
            .iter()
            .copied()
            .find(|&o| e.state.object(o).chars.name == "Fractal")
            .expect("a Fractal token was created");
        assert_eq!(e.state.computed(fractal).power, Some(3), "0/0 + three +1/+1 = 3/3");
        assert_eq!(e.state.computed(fractal).toughness, Some(3));
    }
}
