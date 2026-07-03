//! Rapturous Moment — `{4}{U}{R}` Instant (first printed SOS).
//!
//! Oracle: "Draw three cards, then discard two cards. Add {U}{U}{R}{R}{R}."
//!
//! **Fully implemented** — `Draw 3`, then `Discard 2` (the caster chooses; the `Discard` leaf runs
//! after the draws are flushed to hand), then a mana ritual adding `{U}{U}{R}{R}{R}`. Multicolored (U/R).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::ManaSpec;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const RAPTUROUS_MOMENT: u32 = 228;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Draw {
            who: PlayerRef::Controller,
            count: ValueExpr::Fixed(3),
        },
        Effect::Discard {
            who: PlayerRef::Controller,
            count: ValueExpr::Fixed(2),
        },
        Effect::AddMana {
            who: PlayerRef::Controller,
            mana: ManaSpec {
                produces: vec![(Color::Blue, ValueExpr::Fixed(2)), (Color::Red, ValueExpr::Fixed(3))],
                any_color: None,
            },
        },
    ]);
    let mut def = spell(
        RAPTUROUS_MOMENT,
        "Rapturous Moment",
        CardType::Instant,
        Color::Blue,
        mana_cost(4, &[(Color::Blue, 1), (Color::Red, 1)]),
        effect,
    )
    .with_text("Draw three cards, then discard two cards. Add {U}{U}{R}{R}{R}.");
    def.chars.colors = vec![Color::Blue, Color::Red];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn rapturous_moment_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(RAPTUROUS_MOMENT).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Blue, Color::Red]);
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    Draw {
                        who: Controller,
                        count: Fixed(
                            3,
                        ),
                    },
                    Discard {
                        who: Controller,
                        count: Fixed(
                            2,
                        ),
                    },
                    AddMana {
                        who: Controller,
                        mana: ManaSpec {
                            produces: [
                                (
                                    Blue,
                                    Fixed(
                                        2,
                                    ),
                                ),
                                (
                                    Red,
                                    Fixed(
                                        3,
                                    ),
                                ),
                            ],
                            any_color: None,
                        },
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: draw three then discard two (net +1 in hand), and five mana lands in the pool.
    #[test]
    fn rapturous_moment_draws_discards_and_makes_mana() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        #[derive(Clone)]
        struct DiscardAgent;
        impl Agent for DiscardAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { min, max, from, .. } => {
                        let n = (*min).max(1).min(*max).min(from.len() as u32);
                        DecisionResponse::Indices((0..n).collect())
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        // Library of three Forests to draw.
        let mut state = build_game(1, &[&[grp::FOREST, grp::FOREST, grp::FOREST], &[]]);
        let effect = state.card_db().get(RAPTUROUS_MOMENT).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(DiscardAgent), Box::new(DiscardAgent)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.players[0].hand.len(), 1, "drew three, discarded two → net one in hand");
        assert_eq!(e.state.player(PlayerId(0)).mana_pool.total(), 5, "added five mana ({{U}}{{U}}{{R}}{{R}}{{R}})");
    }
}
