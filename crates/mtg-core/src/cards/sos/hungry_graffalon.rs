//! Hungry Graffalon — `{3}{G}` Creature — Giraffe 3/4 (first printed SOS).
//!
//! Oracle: "Reach / Increment (Whenever you cast a spell, if the amount of mana you spent is greater
//! than this creature's power or toughness, put a +1/+1 counter on this creature.)"
//!
//! **Fully implemented** — printed Reach + the shared Increment cast-trigger.

use crate::basics::Color;
use crate::cards::helpers::increment_ability;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const HUNGRY_GRAFFALON: u32 = 264;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        HUNGRY_GRAFFALON,
        "Hungry Graffalon",
        &[CreatureType::Giraffe],
        Color::Green,
        mana_cost(3, &[(Color::Green, 1)]),
        3,
        4,
        vec![increment_ability()],
    );
    def.chars.keywords = vec![Keyword::Reach];
    def.text = "Reach\nIncrement (Whenever you cast a spell, if the amount of mana you spent is greater than this creature's power or toughness, put a +1/+1 counter on this creature.)".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::ability::Ability;
    use expect_test::expect;

    #[test]
    fn hungry_graffalon_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(HUNGRY_GRAFFALON).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Reach]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SpellCast(
                        Any,
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: Conditional {
                        cond: AnyOf(
                            [
                                ValueAtLeast(
                                    ManaSpentOnTrigger,
                                    Sum(
                                        PowerOfSelf,
                                        Fixed(
                                            1,
                                        ),
                                    ),
                                ),
                                ValueAtLeast(
                                    ManaSpentOnTrigger,
                                    Sum(
                                        ToughnessOfSelf,
                                        Fixed(
                                            1,
                                        ),
                                    ),
                                ),
                            ],
                        ),
                        then: PutCounters {
                            what: SourceSelf,
                            kind: PlusOnePlusOne,
                            n: Fixed(
                                1,
                            ),
                        },
                        otherwise: None,
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: Increment adds a counter only when the mana spent exceeds power OR toughness
    /// (3/4 → a 4-mana spell grows it since 4 > 3; a 3-mana spell does not, since 3 ≯ 3 and 3 ≯ 4).
    #[test]
    fn hungry_graffalon_increment_gated_on_stats() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        let power_after = |mana_spent: u32| {
            let mut state = build_game(1, &[&[], &[]]);
            let src = {
                let c = state.card_db().get(HUNGRY_GRAFFALON).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            let spell = {
                let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Stack)
            };
            state.objects.get_mut(&spell).unwrap().mana_spent = mana_spent;
            let eff = match &state.card_db().get(HUNGRY_GRAFFALON).unwrap().abilities[0] {
                Ability::Triggered { effect, .. } => effect.clone(),
                o => panic!("expected Increment Triggered, got {o:?}"),
            };
            let mut e = Engine::new(
                state,
                vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
            );
            e.resolve_effect(
                &eff,
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
        assert_eq!(power_after(3), 3, "3 mana ≯ power 3 and ≯ toughness 4 → no counter");
        assert_eq!(power_after(4), 4, "4 mana > power 3 → +1/+1 counter → 4/5");
    }
}
