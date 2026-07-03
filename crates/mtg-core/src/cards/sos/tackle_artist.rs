//! Tackle Artist — `{3}{R}` Creature — Orc Sorcerer 4/3 (first printed SOS).
//!
//! Oracle: "Trample / Opus — Whenever you cast an instant or sorcery spell, put a +1/+1 counter on
//! this creature. If five or more mana was spent to cast that spell, put two +1/+1 counters on this
//! creature instead."
//!
//! **Fully implemented** — printed Trample + an Opus cast-trigger: a `Conditional` on
//! `ManaSpentOnTrigger ≥ 5` putting two +1/+1 counters on itself, else one.

use crate::basics::{Color, CounterKind};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Condition;
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const TACKLE_ARTIST: u32 = 255;

fn put(n: i64) -> Effect {
    Effect::PutCounters {
        what: EffectTarget::SourceSelf,
        kind: CounterKind::PlusOnePlusOne,
        n: ValueExpr::Fixed(n),
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        TACKLE_ARTIST,
        "Tackle Artist",
        &[CreatureType::Orc, CreatureType::Sorcerer],
        Color::Red,
        mana_cost(3, &[(Color::Red, 1)]),
        4,
        3,
        vec![Ability::Triggered {
            event: EventPattern::SpellCast(instant_or_sorcery()),
            condition: None,
            intervening_if: false,
            effect: Effect::Conditional {
                cond: Condition::ValueAtLeast(ValueExpr::ManaSpentOnTrigger, ValueExpr::Fixed(5)),
                then: Box::new(put(2)),
                otherwise: Some(Box::new(put(1))),
            },
        }],
    );
    def.chars.keywords = vec![Keyword::Trample];
    def.text = "Trample\nOpus — Whenever you cast an instant or sorcery spell, put a +1/+1 counter on this creature. If five or more mana was spent to cast that spell, put two +1/+1 counters on this creature instead.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn tackle_artist_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TACKLE_ARTIST).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Trample]);
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
                                2,
                            ),
                        },
                        otherwise: Some(
                            PutCounters {
                                what: SourceSelf,
                                kind: PlusOnePlusOne,
                                n: Fixed(
                                    1,
                                ),
                            },
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: a cheap Opus trigger adds one counter (→5/4); a 5-mana one adds two (→6/5).
    #[test]
    fn tackle_artist_opus_scales() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        let power_after = |mana_spent: u32| {
            let mut state = build_game(1, &[&[], &[]]);
            let src = {
                let c = state.card_db().get(TACKLE_ARTIST).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            let spell = {
                let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Stack)
            };
            state.objects.get_mut(&spell).unwrap().mana_spent = mana_spent;
            let etb = match &state.card_db().get(TACKLE_ARTIST).unwrap().abilities[0] {
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
            e.state.computed(src).power.unwrap()
        };
        assert_eq!(power_after(3), 5, "cheap → one +1/+1 counter → 5/4");
        assert_eq!(power_after(5), 6, "5+ mana → two +1/+1 counters → 6/5");
    }
}
