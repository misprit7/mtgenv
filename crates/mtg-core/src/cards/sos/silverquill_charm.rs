//! Silverquill Charm — `{W}{B}` Instant (first printed SOS).
//!
//! Oracle: "Choose one —
//!   • Put two +1/+1 counters on target creature.
//!   • Exile target creature with power 2 or less.
//!   • Each opponent loses 3 life and you gain 3 life."
//!
//! **Fully implemented** — a `Modal` "choose one" (CR 700.2). Only the chosen mode's target is
//! collected at cast. Mode 2 uses the `PowerAtMost(2)` filter (CR — "power 2 or less"); mode 3 is a
//! pure drain (`EachOpponent` loses 3, controller gains 3). Multicolored (W/B).

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, Mode};

/// grp id (per-set ids live near their cards).
pub const SILVERQUILL_CHARM: u32 = 210;

fn creature_target(filter: CardFilter) -> EffectTarget {
    EffectTarget::Target(TargetSpec {
        kind: TargetKind::Creature(filter),
        min: 1,
        max: 1,
        distinct: true,
    })
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "Put two +1/+1 counters on target creature".to_string(),
                effect: Effect::PutCounters {
                    what: creature_target(CardFilter::Any),
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(2),
                },
            },
            Mode {
                label: "Exile target creature with power 2 or less".to_string(),
                effect: Effect::Exile {
                    what: creature_target(CardFilter::PowerAtMost(2)),
                },
            },
            Mode {
                label: "Each opponent loses 3 life and you gain 3 life".to_string(),
                effect: Effect::Sequence(vec![
                    Effect::LoseLife {
                        who: PlayerRef::EachOpponent,
                        amount: ValueExpr::Fixed(3),
                    },
                    Effect::GainLife {
                        who: PlayerRef::Controller,
                        amount: ValueExpr::Fixed(3),
                    },
                ]),
            },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    let mut def = spell(
        SILVERQUILL_CHARM,
        "Silverquill Charm",
        CardType::Instant,
        Color::White,
        mana_cost(0, &[(Color::White, 1), (Color::Black, 1)]),
        effect,
    )
    .with_text("Choose one —\n• Put two +1/+1 counters on target creature.\n• Exile target creature with power 2 or less.\n• Each opponent loses 3 life and you gain 3 life.");
    def.chars.colors = vec![Color::White, Color::Black];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn silverquill_charm_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SILVERQUILL_CHARM).unwrap();
        assert_eq!(def.chars.colors, vec![Color::White, Color::Black]);
        assert!(def.fully_implemented);
        expect![[r#"
            Modal {
                modes: [
                    Mode {
                        label: "Put two +1/+1 counters on target creature",
                        effect: PutCounters {
                            what: Target(
                                TargetSpec {
                                    kind: Creature(
                                        Any,
                                    ),
                                    min: 1,
                                    max: 1,
                                    distinct: true,
                                },
                            ),
                            kind: PlusOnePlusOne,
                            n: Fixed(
                                2,
                            ),
                        },
                    },
                    Mode {
                        label: "Exile target creature with power 2 or less",
                        effect: Exile {
                            what: Target(
                                TargetSpec {
                                    kind: Creature(
                                        PowerAtMost(
                                            2,
                                        ),
                                    ),
                                    min: 1,
                                    max: 1,
                                    distinct: true,
                                },
                            ),
                        },
                    },
                    Mode {
                        label: "Each opponent loses 3 life and you gain 3 life",
                        effect: Sequence(
                            [
                                LoseLife {
                                    who: EachOpponent,
                                    amount: Fixed(
                                        3,
                                    ),
                                },
                                GainLife {
                                    who: Controller,
                                    amount: Fixed(
                                        3,
                                    ),
                                },
                            ],
                        ),
                    },
                ],
                min: 1,
                max: 1,
                allow_repeat: false,
            }"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: choosing mode 0 puts two +1/+1 counters on the target (a 2/2 → 4/4); choosing
    /// mode 2 drains the opponent for 3 and gains the caster 3.
    #[test]
    fn silverquill_charm_modes_resolve() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let creature = state.add_card(PlayerId(0), bears, Zone::Battlefield);
        let effect = state.card_db().get(SILVERQUILL_CHARM).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        // Mode 0: two +1/+1 counters → 4/4.
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_modes: vec![0],
                chosen_targets: vec![Target::Object(creature)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.computed(creature).power, Some(4), "mode 0: +2/+2 from two counters");
        // Mode 2: drain 3.
        let p0 = e.state.player(PlayerId(0)).life;
        let p1 = e.state.player(PlayerId(1)).life;
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_modes: vec![2],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(1)).life, p1 - 3, "mode 2: opponent loses 3");
        assert_eq!(e.state.player(PlayerId(0)).life, p0 + 3, "mode 2: you gain 3");
    }
}
