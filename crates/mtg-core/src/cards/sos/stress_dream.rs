//! Stress Dream — `{3}{U}{R}` Instant (first printed SOS).
//!
//! Oracle: "Stress Dream deals 5 damage to up to one target creature. Look at the top two cards of
//! your library. Put one of those cards into your hand and the other on the bottom of your library."
//!
//! **Fully implemented** — 5 damage to up to one creature, then a `LookAndPick` (look 2, keep 1 to
//! hand, the other to the bottom). Multicolored (U/R).

use crate::basics::{CardType, Color, DamageKind, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const STRESS_DREAM: u32 = 279;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::DealDamage {
            amount: ValueExpr::Fixed(5),
            to: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 0,
                max: 1,
                distinct: true,
            }),
            kind: DamageKind::Noncombat,
        },
        Effect::LookAndPick {
            count: ValueExpr::Fixed(2),
            take: ValueExpr::Fixed(1),
            take_to: Zone::Hand,
            rest_to: Zone::Library,
            take_filter: CardFilter::Any,
        },
    ]);
    let mut def = spell(
        STRESS_DREAM,
        "Stress Dream",
        CardType::Instant,
        Color::Blue,
        mana_cost(3, &[(Color::Blue, 1), (Color::Red, 1)]),
        effect,
    )
    .with_text("Stress Dream deals 5 damage to up to one target creature. Look at the top two cards of your library. Put one of those cards into your hand and the other on the bottom of your library.");
    def.chars.colors = vec![Color::Blue, Color::Red];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn stress_dream_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(STRESS_DREAM).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Blue, Color::Red]);
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    DealDamage {
                        amount: Fixed(
                            5,
                        ),
                        to: Target(
                            TargetSpec {
                                kind: Creature(
                                    Any,
                                ),
                                min: 0,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        kind: Noncombat,
                    },
                    LookAndPick {
                        count: Fixed(
                            2,
                        ),
                        take: Fixed(
                            1,
                        ),
                        take_to: Hand,
                        rest_to: Library,
                        take_filter: Any,
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour (look-and-pick half): with a known 2-card top, keep one card to hand and bottom the
    /// other. The kept card leaves the library; the library size drops by exactly one.
    #[test]
    fn stress_dream_look_and_pick_keeps_one() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        #[derive(Clone)] struct KeepFirst;
        impl Agent for KeepFirst {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req { DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![0]), _ => DecisionResponse::Pass }
            }
        }
        // Library (bottom→top): Forest, Grizzly, Island. Top two = [Island, Grizzly].
        let lib = vec![grp::FOREST, grp::GRIZZLY_BEARS, grp::ISLAND];
        let state = build_game(1, &[&lib, &[]]);
        // Only the look-and-pick node (skip the damage; no target).
        let effect = Effect::LookAndPick { count: ValueExpr::Fixed(2), take: ValueExpr::Fixed(1), take_to: Zone::Hand, rest_to: Zone::Library, take_filter: CardFilter::Any };
        let mut e = Engine::new(state, vec![Box::new(KeepFirst), Box::new(KeepFirst)]);
        let (lib0, hand0) = (e.state.players[0].library.len(), e.state.players[0].hand.len());
        e.resolve_effect(&effect, &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.players[0].hand.len(), hand0 + 1, "kept one card to hand");
        assert_eq!(e.state.players[0].library.len(), lib0 - 1, "one card left the library; the other went to the bottom");
        // The other looked-at card is now on the bottom (front of the vec).
        assert_eq!(e.state.players[0].library.len(), 2);
    }
}
