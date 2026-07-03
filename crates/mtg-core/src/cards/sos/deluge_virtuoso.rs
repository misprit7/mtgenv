//! Deluge Virtuoso — `{2}{U}` Creature — Human Wizard 2/2 (first printed SOS).
//!
//! Oracle: "When this creature enters, tap target creature an opponent controls and put a stun
//! counter on it. / Opus — Whenever you cast an instant or sorcery spell, this creature gets +1/+1
//! until end of turn. If five or more mana was spent to cast that spell, this creature gets +2/+2
//! until end of turn instead."
//!
//! **Fully implemented** — an ETB tap-and-stun (target opponent creature) plus an Opus cast-trigger
//! that pumps itself +1/+1, or +2/+2 when `ManaSpentOnTrigger ≥ 5`.

use crate::basics::{Color, CounterKind};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const DELUGE_VIRTUOSO: u32 = 283;

fn pump(n: i64) -> Effect {
    Effect::PumpPT {
        what: EffectTarget::SourceSelf,
        power: ValueExpr::Fixed(n),
        toughness: ValueExpr::Fixed(n),
        duration: Duration::UntilEndOfTurn,
    }
}

pub fn register(db: &mut CardDb) {
    let etb = Effect::Sequence(vec![
        Effect::Tap {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Opponent)),
                min: 1,
                max: 1,
                distinct: true,
            }),
            tap: true,
        },
        Effect::PutCounters { what: EffectTarget::ChosenIndex(0), kind: CounterKind::Stun, n: ValueExpr::Fixed(1) },
    ]);
    db.insert(
        creature(
            DELUGE_VIRTUOSO,
            "Deluge Virtuoso",
            &[CreatureType::Human, CreatureType::Wizard],
            Color::Blue,
            mana_cost(2, &[(Color::Blue, 1)]),
            2,
            2,
            vec![
                Ability::Triggered { event: EventPattern::SelfEnters, condition: None, intervening_if: false, effect: etb },
                Ability::Triggered {
                    event: EventPattern::SpellCast(instant_or_sorcery()),
                    condition: None,
                    intervening_if: false,
                    effect: Effect::Conditional {
                        cond: Condition::ValueAtLeast(ValueExpr::ManaSpentOnTrigger, ValueExpr::Fixed(5)),
                        then: Box::new(pump(2)),
                        otherwise: Some(Box::new(pump(1))),
                    },
                },
            ],
        )
        .with_text("When this creature enters, tap target creature an opponent controls and put a stun counter on it.\nOpus — Whenever you cast an instant or sorcery spell, this creature gets +1/+1 until end of turn. If five or more mana was spent to cast that spell, this creature gets +2/+2 until end of turn instead."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn deluge_virtuoso_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        assert!(db.get(DELUGE_VIRTUOSO).unwrap().fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Sequence(
                        [
                            Tap {
                                what: Target(
                                    TargetSpec {
                                        kind: Creature(
                                            ControlledBy(
                                                Opponent,
                                            ),
                                        ),
                                        min: 1,
                                        max: 1,
                                        distinct: true,
                                    },
                                ),
                                tap: true,
                            },
                            PutCounters {
                                what: ChosenIndex(
                                    0,
                                ),
                                kind: Stun,
                                n: Fixed(
                                    1,
                                ),
                            },
                        ],
                    ),
                },
                Triggered {
                    event: SpellCast(
                        AnyOf(
                            [
                                HasCardType(
                                    Instant,
                                ),
                                HasCardType(
                                    Sorcery,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: Conditional {
                        cond: ValueAtLeast(
                            ManaSpentOnTrigger,
                            Fixed(
                                5,
                            ),
                        ),
                        then: PumpPT {
                            what: SourceSelf,
                            power: Fixed(
                                2,
                            ),
                            toughness: Fixed(
                                2,
                            ),
                            duration: UntilEndOfTurn,
                        },
                        otherwise: Some(
                            PumpPT {
                                what: SourceSelf,
                                power: Fixed(
                                    1,
                                ),
                                toughness: Fixed(
                                    1,
                                ),
                                duration: UntilEndOfTurn,
                            },
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", db.get(DELUGE_VIRTUOSO).unwrap().abilities));
    }

    #[test]
    fn deluge_virtuoso_opus_pumps_scaling() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let pow = |mana: u32| {
            let mut state = build_game(1, &[&[], &[]]);
            let src = state.add_card(PlayerId(0), state.card_db().get(DELUGE_VIRTUOSO).unwrap().chars.clone(), Zone::Battlefield);
            let spell = state.add_card(PlayerId(0), state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone(), Zone::Stack);
            state.objects.get_mut(&spell).unwrap().mana_spent = mana;
            let eff = match &state.card_db().get(DELUGE_VIRTUOSO).unwrap().abilities[1] {
                Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
            let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
            e.resolve_effect(&eff, &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), triggering_spell: Some(spell), ..Default::default() }, WbReason::Resolve(StackId(0)));
            e.state.computed(src).power.unwrap()
        };
        assert_eq!(pow(3), 3, "cheap → +1/+1 → 3/3");
        assert_eq!(pow(5), 4, "5+ mana → +2/+2 → 4/4");
    }
}
