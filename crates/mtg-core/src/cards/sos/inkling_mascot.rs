//! Inkling Mascot — `{W}{B}` Creature — Inkling Cat 2/2 (first printed SOS).
//!
//! Oracle: "Repartee — Whenever you cast an instant or sorcery spell that targets a creature, this
//! creature gains flying until end of turn. Surveil 1."
//!
//! **Fully implemented** — a Repartee cast-trigger granting itself flying until end of turn, then
//! Surveil 1. Multicolored (W/B).

use crate::basics::Color;
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Duration;
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const INKLING_MASCOT: u32 = 268;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        INKLING_MASCOT,
        "Inkling Mascot",
        &[CreatureType::Inkling, CreatureType::Cat],
        Color::White,
        mana_cost(0, &[(Color::White, 1), (Color::Black, 1)]),
        2,
        2,
        vec![Ability::Triggered {
            event: EventPattern::SpellCastTargetingCreature(instant_or_sorcery()),
            condition: None,
            intervening_if: false,
            effect: Effect::Sequence(vec![
                Effect::GrantKeyword {
                    what: EffectTarget::SourceSelf,
                    keyword: Keyword::Flying,
                    duration: Duration::UntilEndOfTurn,
                },
                Effect::Surveil { count: ValueExpr::Fixed(1) },
            ]),
        }],
    );
    def.chars.colors = vec![Color::White, Color::Black];
    def.text = "Repartee — Whenever you cast an instant or sorcery spell that targets a creature, this creature gains flying until end of turn. Surveil 1.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn inkling_mascot_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(INKLING_MASCOT).unwrap();
        assert_eq!(def.chars.colors, vec![Color::White, Color::Black]);
        assert!(def.fully_implemented);
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
                            GrantKeyword {
                                what: SourceSelf,
                                keyword: Flying,
                                duration: UntilEndOfTurn,
                            },
                            Surveil {
                                count: Fixed(
                                    1,
                                ),
                            },
                        ],
                    ),
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    #[test]
    fn inkling_mascot_repartee_grants_flying() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        #[derive(Clone)] struct KeepAll;
        impl Agent for KeepAll {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req { DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![]), _ => DecisionResponse::Pass }
            }
        }
        let mut state = build_game(1, &[&[], &[]]);
        let src = state.add_card(PlayerId(0), state.card_db().get(INKLING_MASCOT).unwrap().chars.clone(), Zone::Battlefield);
        let eff = match &state.card_db().get(INKLING_MASCOT).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
        let mut e = Engine::new(state, vec![Box::new(KeepAll), Box::new(KeepAll)]);
        assert!(!e.state.computed(src).has_keyword(Keyword::Flying));
        e.resolve_effect(&eff, &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert!(e.state.computed(src).has_keyword(Keyword::Flying), "gained flying until EOT");
    }
}
