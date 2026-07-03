//! Muse's Encouragement — `{4}{U}` Instant (first printed SOS).
//!
//! Oracle: "Create a 3/3 blue and red Elemental creature token with flying. Surveil 2."
//!
//! **Fully implemented** — `CreateToken` of the shared Elemental token (a 3/3 U/R flyer; its Flying
//! now lands via token keywords), then `Surveil 2`.

use crate::basics::{CardType, Color};
use crate::cards::helpers::elemental_token;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const MUSES_ENCOURAGEMENT: u32 = 235;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::CreateToken {
            spec: elemental_token(),
            count: ValueExpr::Fixed(1),
            controller: PlayerRef::Controller,
        },
        Effect::Surveil { count: ValueExpr::Fixed(2) },
    ]);
    db.insert(
        spell(
            MUSES_ENCOURAGEMENT,
            "Muse's Encouragement",
            CardType::Instant,
            Color::Blue,
            mana_cost(4, &[(Color::Blue, 1)]),
            effect,
        )
        .with_text("Create a 3/3 blue and red Elemental creature token with flying. Surveil 2."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn muses_encouragement_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MUSES_ENCOURAGEMENT).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    CreateToken {
                        spec: TokenSpec {
                            name: "Elemental",
                            card_types: [
                                Creature,
                            ],
                            subtypes: [
                                Creature(
                                    Elemental,
                                ),
                            ],
                            colors: [
                                Blue,
                                Red,
                            ],
                            power: 3,
                            toughness: 3,
                            keywords: [
                                Flying,
                            ],
                            counters: [],
                        },
                        count: Fixed(
                            1,
                        ),
                        controller: Controller,
                    },
                    Surveil {
                        count: Fixed(
                            2,
                        ),
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: creates a 3/3 flying Elemental token.
    #[test]
    fn muses_encouragement_makes_a_flying_elemental() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::ability::Keyword;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        #[derive(Clone)]
        struct KeepAll;
        impl Agent for KeepAll {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![]),
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = build_game(1, &[&[grp::FOREST, grp::FOREST], &[]]);
        let effect = state.card_db().get(MUSES_ENCOURAGEMENT).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(KeepAll), Box::new(KeepAll)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let tok = e.state.players[0]
            .battlefield
            .iter()
            .copied()
            .find(|&o| e.state.object(o).chars.name == "Elemental")
            .expect("an Elemental token was created");
        assert_eq!(e.state.computed(tok).power, Some(3));
        assert!(e.state.computed(tok).has_keyword(Keyword::Flying), "the Elemental has flying");
    }
}
