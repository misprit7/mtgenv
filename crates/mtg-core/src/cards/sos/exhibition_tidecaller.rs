//! Exhibition Tidecaller — `{U}` Creature — Djinn Wizard 0/2 (first printed SOS).
//!
//! Oracle: "Opus — Whenever you cast an instant or sorcery spell, target player mills three cards.
//! If five or more mana was spent to cast that spell, that player mills ten cards instead."
//!
//! **Fully implemented** — an Opus cast-trigger with a **target player** (`TargetPlayer`) that mills
//! three, or ten when `ManaSpentOnTrigger ≥ 5` (both via `PlayerRef::ChosenTarget(0)`).

use crate::basics::Color;
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::target::PlayerFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const EXHIBITION_TIDECALLER: u32 = 272;

fn mill(n: i64) -> Effect {
    Effect::Mill { who: PlayerRef::ChosenTarget(0), count: ValueExpr::Fixed(n) }
}

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            EXHIBITION_TIDECALLER,
            "Exhibition Tidecaller",
            &[CreatureType::Djinn, CreatureType::Wizard],
            Color::Blue,
            mana_cost(0, &[(Color::Blue, 1)]),
            0,
            2,
            vec![Ability::Triggered {
                event: EventPattern::SpellCast(instant_or_sorcery()),
                condition: None,
                intervening_if: false,
                effect: Effect::Sequence(vec![
                    Effect::TargetPlayer(PlayerFilter::Any),
                    Effect::Conditional {
                        cond: Condition::ValueAtLeast(ValueExpr::ManaSpentOnTrigger, ValueExpr::Fixed(5)),
                        then: Box::new(mill(10)),
                        otherwise: Some(Box::new(mill(3))),
                    },
                ]),
            }],
        )
        .with_text("Opus — Whenever you cast an instant or sorcery spell, target player mills three cards. If five or more mana was spent to cast that spell, that player mills ten cards instead."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn exhibition_tidecaller_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        assert!(db.get(EXHIBITION_TIDECALLER).unwrap().fully_implemented);
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
                            TargetPlayer(
                                Any,
                            ),
                            Conditional {
                                cond: ValueAtLeast(
                                    ManaSpentOnTrigger,
                                    Fixed(
                                        5,
                                    ),
                                ),
                                then: Mill {
                                    who: ChosenTarget(
                                        0,
                                    ),
                                    count: Fixed(
                                        10,
                                    ),
                                },
                                otherwise: Some(
                                    Mill {
                                        who: ChosenTarget(
                                            0,
                                        ),
                                        count: Fixed(
                                            3,
                                        ),
                                    },
                                ),
                            },
                        ],
                    ),
                },
            ]"#]].assert_eq(&format!("{:#?}", db.get(EXHIBITION_TIDECALLER).unwrap().abilities));
    }

    /// Behaviour: the targeted player mills 3 on a cheap spell, 10 on a 5-mana spell.
    #[test]
    fn exhibition_tidecaller_opus_mills_scaling() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let milled = |mana_spent: u32| {
            // Target player (P1) has a 12-card library.
            let lib: Vec<u32> = std::iter::repeat(grp::FOREST).take(12).collect();
            let mut state = build_game(1, &[&[], &lib]);
            let src = state.add_card(PlayerId(0), state.card_db().get(EXHIBITION_TIDECALLER).unwrap().chars.clone(), Zone::Battlefield);
            let spell = state.add_card(PlayerId(0), state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone(), Zone::Stack);
            state.objects.get_mut(&spell).unwrap().mana_spent = mana_spent;
            let eff = match &state.card_db().get(EXHIBITION_TIDECALLER).unwrap().abilities[0] {
                Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
            let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
            e.resolve_effect(&eff, &ResolutionCtx {
                controller: Some(PlayerId(0)), source: Some(src), triggering_spell: Some(spell),
                chosen_targets: vec![Target::Player(PlayerId(1))], ..Default::default()
            }, WbReason::Resolve(StackId(0)));
            e.state.players[1].graveyard.len()
        };
        assert_eq!(milled(3), 3, "cheap → target player mills 3");
        assert_eq!(milled(5), 10, "5+ mana → target player mills 10");
    }
}
