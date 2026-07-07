//! Molten-Core Maestro — `{1}{R}` Creature — Goblin Bard 2/2 (first printed SOS).
//!
//! Oracle: "Menace / Opus — Whenever you cast an instant or sorcery spell, put a +1/+1 counter on
//! this creature. If five or more mana was spent to cast that spell, add an amount of {R} equal to
//! this creature's power."
//!
//! **Fully implemented** — printed Menace + an Opus cast-trigger: always a +1/+1 counter on itself,
//! and (when `ManaSpentOnTrigger ≥ 5`) also add `{R}` equal to its power (`PowerOfSelf`, read after
//! the counter lands).

use crate::basics::Color;
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Condition;
use crate::effects::target::ManaSpec;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::basics::CounterKind;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const MOLTEN_CORE_MAESTRO: u32 = 273;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        MOLTEN_CORE_MAESTRO,
        "Molten-Core Maestro",
        &[CreatureType::Goblin, CreatureType::Bard],
        Color::Red,
        mana_cost(1, &[(Color::Red, 1)]),
        2,
        2,
        vec![Ability::Triggered {
            event: EventPattern::SpellCast(instant_or_sorcery()),
            condition: None,
            intervening_if: false,
            effect: Effect::Sequence(vec![
                Effect::PutCounters {
                    what: EffectTarget::SourceSelf,
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(1),
                },
                Effect::Conditional {
                    cond: Condition::ValueAtLeast(ValueExpr::ManaSpentOnTrigger, ValueExpr::Fixed(5)),
                    then: Box::new(Effect::AddMana {
                        who: PlayerRef::Controller,
                        mana: ManaSpec {
                            produces: vec![(Color::Red, ValueExpr::PowerOfSelf)],
                            any_color: None,
                            one_of: None,
                            restriction: None,
                        },
                    }),
                    otherwise: None,
                },
            ]),
        }],
    );
    def.chars.keywords = vec![Keyword::Menace];
    def.text = "Menace\nOpus — Whenever you cast an instant or sorcery spell, put a +1/+1 counter on this creature. If five or more mana was spent to cast that spell, add an amount of {R} equal to this creature's power.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn molten_core_maestro_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MOLTEN_CORE_MAESTRO).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Menace]);
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
                    effect: Sequence(
                        [
                            PutCounters {
                                what: SourceSelf,
                                kind: PlusOnePlusOne,
                                n: Fixed(
                                    1,
                                ),
                            },
                            Conditional {
                                cond: ValueAtLeast(
                                    ManaSpentOnTrigger,
                                    Fixed(
                                        5,
                                    ),
                                ),
                                then: AddMana {
                                    who: Controller,
                                    mana: ManaSpec {
                                        produces: [
                                            (
                                                Red,
                                                PowerOfSelf,
                                            ),
                                        ],
                                        any_color: None,
                                        one_of: None,
                                        restriction: None,
                                    },
                                },
                                otherwise: None,
                            },
                        ],
                    ),
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: always a +1/+1 counter; a 5-mana spell also adds {R} equal to the new power.
    #[test]
    fn molten_core_maestro_opus() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let run = |mana_spent: u32| {
            let mut state = build_game(1, &[&[], &[]]);
            let src = state.add_card(PlayerId(0), state.card_db().get(MOLTEN_CORE_MAESTRO).unwrap().chars.clone(), Zone::Battlefield);
            let spell = state.add_card(PlayerId(0), state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone(), Zone::Stack);
            state.objects.get_mut(&spell).unwrap().mana_spent = mana_spent;
            let eff = match &state.card_db().get(MOLTEN_CORE_MAESTRO).unwrap().abilities[0] {
                Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
            let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
            e.resolve_effect(&eff, &ResolutionCtx {
                controller: Some(PlayerId(0)), source: Some(src), triggering_spell: Some(spell), ..Default::default()
            }, WbReason::Resolve(StackId(0)));
            (e.state.computed(src).power.unwrap(), e.state.player(PlayerId(0)).mana_pool.total())
        };
        assert_eq!(run(3), (3, 0), "cheap → +1/+1 (3/3), no mana");
        assert_eq!(run(5), (3, 3), "5+ mana → +1/+1 (3/3) and add {{R}} equal to power (3)");
    }
}
