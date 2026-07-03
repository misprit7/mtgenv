//! Spectacular Skywhale — `{2}{U}{R}` Creature — Elemental Whale 1/4 (first printed SOS).
//!
//! Oracle: "Flying / Opus — Whenever you cast an instant or sorcery spell, this creature gets +3/+0
//! until end of turn. If five or more mana was spent to cast that spell, put three +1/+1 counters on
//! this creature instead."
//!
//! **Fully implemented** — printed Flying + an Opus cast-trigger: `Conditional` on
//! `ManaSpentOnTrigger ≥ 5` — three +1/+1 counters, else a +3/+0 until end of turn. Multicolored (U/R).

use crate::basics::{Color, CounterKind};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::{Condition, Duration};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const SPECTACULAR_SKYWHALE: u32 = 256;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        SPECTACULAR_SKYWHALE,
        "Spectacular Skywhale",
        &[CreatureType::Elemental, CreatureType::Whale],
        Color::Blue,
        mana_cost(2, &[(Color::Blue, 1), (Color::Red, 1)]),
        1,
        4,
        vec![Ability::Triggered {
            event: EventPattern::SpellCast(instant_or_sorcery()),
            condition: None,
            intervening_if: false,
            effect: Effect::Conditional {
                cond: Condition::ValueAtLeast(ValueExpr::ManaSpentOnTrigger, ValueExpr::Fixed(5)),
                then: Box::new(Effect::PutCounters {
                    what: EffectTarget::SourceSelf,
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(3),
                }),
                otherwise: Some(Box::new(Effect::PumpPT {
                    what: EffectTarget::SourceSelf,
                    power: ValueExpr::Fixed(3),
                    toughness: ValueExpr::Fixed(0),
                    duration: Duration::UntilEndOfTurn,
                })),
            },
        }],
    );
    def.chars.colors = vec![Color::Blue, Color::Red];
    def.chars.keywords = vec![Keyword::Flying];
    def.text = "Flying\nOpus — Whenever you cast an instant or sorcery spell, this creature gets +3/+0 until end of turn. If five or more mana was spent to cast that spell, put three +1/+1 counters on this creature instead.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn spectacular_skywhale_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SPECTACULAR_SKYWHALE).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Blue, Color::Red]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
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
                        then: PutCounters {
                            what: SourceSelf,
                            kind: PlusOnePlusOne,
                            n: Fixed(
                                3,
                            ),
                        },
                        otherwise: Some(
                            PumpPT {
                                what: SourceSelf,
                                power: Fixed(
                                    3,
                                ),
                                toughness: Fixed(
                                    0,
                                ),
                                duration: UntilEndOfTurn,
                            },
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: cheap Opus → +3/+0 EOT (4/4, wears off); 5-mana → three counters (permanent 4/7).
    #[test]
    fn spectacular_skywhale_opus_branches() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        let pt_after = |mana_spent: u32| {
            let mut state = build_game(1, &[&[], &[]]);
            let src = {
                let c = state.card_db().get(SPECTACULAR_SKYWHALE).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            let spell = {
                let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Stack)
            };
            state.objects.get_mut(&spell).unwrap().mana_spent = mana_spent;
            let etb = match &state.card_db().get(SPECTACULAR_SKYWHALE).unwrap().abilities[0] {
                Ability::Triggered { effect, .. } => effect.clone(),
                o => panic!("expected Opus Triggered, got {o:?}"),
            };
            let mut e = Engine::new(
                state,
                vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
            );
            e.resolve_effect(
                &etb,
                &ResolutionCtx {
                    controller: Some(PlayerId(0)),
                    source: Some(src),
                    triggering_spell: Some(spell),
                    ..Default::default()
                },
                WbReason::Resolve(StackId(0)),
            );
            let cc = e.state.computed(src);
            (cc.power, cc.toughness)
        };
        assert_eq!(pt_after(3), (Some(4), Some(4)), "cheap → +3/+0 → 4/4");
        assert_eq!(pt_after(5), (Some(4), Some(7)), "5+ mana → three +1/+1 counters → 4/7");
    }
}
