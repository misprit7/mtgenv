//! Melancholic Poet — `{1}{B}` Creature — Elf Bard 2/2 (first printed SOS).
//!
//! Oracle: "Repartee — Whenever you cast an instant or sorcery spell that targets a creature, each
//! opponent loses 1 life and you gain 1 life."
//!
//! **Fully implemented** — a Repartee cast-trigger draining each opponent for 1 and gaining 1.

use crate::basics::Color;
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const MELANCHOLIC_POET: u32 = 260;

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            MELANCHOLIC_POET,
            "Melancholic Poet",
            &[CreatureType::Elf, CreatureType::Bard],
            Color::Black,
            mana_cost(1, &[(Color::Black, 1)]),
            2,
            2,
            vec![Ability::Triggered {
                event: EventPattern::SpellCastTargetingCreature(instant_or_sorcery()),
                condition: None,
                intervening_if: false,
                effect: Effect::Sequence(vec![
                    Effect::LoseLife { who: PlayerRef::EachOpponent, amount: ValueExpr::Fixed(1) },
                    Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
                ]),
            }],
        )
        .with_text("Repartee — Whenever you cast an instant or sorcery spell that targets a creature, each opponent loses 1 life and you gain 1 life."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn melancholic_poet_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        assert!(db.get(MELANCHOLIC_POET).unwrap().fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SpellCastTargetingCreature(
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
                            LoseLife {
                                who: EachOpponent,
                                amount: Fixed(
                                    1,
                                ),
                            },
                            GainLife {
                                who: Controller,
                                amount: Fixed(
                                    1,
                                ),
                            },
                        ],
                    ),
                },
            ]"#]].assert_eq(&format!("{:#?}", db.get(MELANCHOLIC_POET).unwrap().abilities));
    }

    #[test]
    fn melancholic_poet_repartee_drains() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let src = state.add_card(PlayerId(0), state.card_db().get(MELANCHOLIC_POET).unwrap().chars.clone(), Zone::Battlefield);
        let eff = match &state.card_db().get(MELANCHOLIC_POET).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let (p0, p1) = (e.state.player(PlayerId(0)).life, e.state.player(PlayerId(1)).life);
        e.resolve_effect(&eff, &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.player(PlayerId(1)).life, p1 - 1);
        assert_eq!(e.state.player(PlayerId(0)).life, p0 + 1);
    }
}
