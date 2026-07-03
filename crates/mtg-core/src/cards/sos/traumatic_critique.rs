//! Traumatic Critique — `{X}{U}{R}` Instant (first printed SOS).
//!
//! Oracle: "Traumatic Critique deals X damage to any target. Draw two cards, then discard a card."
//!
//! **Fully implemented** — `DealDamage X` to one "any target" (CR 115.4), then `Draw 2` +
//! `Discard 1` for the caster. The discard exercises the `Effect::Discard` leaf (a card-agnostic
//! engine cap): after the (flushed) draws, the caster chooses one card in hand to discard.
//! Multicolored (U/R); one `{X}` in the cost (`cost.x = 1`), read as `ValueExpr::X` at resolution.

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const TRAUMATIC_CRITIQUE: u32 = 216;

pub fn register(db: &mut CardDb) {
    let mut cost = mana_cost(0, &[(Color::Blue, 1), (Color::Red, 1)]);
    cost.x = 1;
    let effect = Effect::Sequence(vec![
        Effect::DealDamage {
            amount: ValueExpr::X,
            to: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Any,
                min: 1,
                max: 1,
                distinct: true,
            }),
            kind: DamageKind::Noncombat,
        },
        Effect::Draw {
            who: PlayerRef::Controller,
            count: ValueExpr::Fixed(2),
        },
        Effect::Discard {
            who: PlayerRef::Controller,
            count: ValueExpr::Fixed(1),
        },
    ]);
    let mut def = spell(
        TRAUMATIC_CRITIQUE,
        "Traumatic Critique",
        CardType::Instant,
        Color::Blue,
        cost,
        effect,
    )
    .with_text("Traumatic Critique deals X damage to any target. Draw two cards, then discard a card.");
    def.chars.colors = vec![Color::Blue, Color::Red];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn traumatic_critique_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TRAUMATIC_CRITIQUE).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Blue, Color::Red]);
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().x, 1, "one {{X}}");
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    DealDamage {
                        amount: X,
                        to: Target(
                            TargetSpec {
                                kind: Any,
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        kind: Noncombat,
                    },
                    Draw {
                        who: Controller,
                        count: Fixed(
                            2,
                        ),
                    },
                    Discard {
                        who: Controller,
                        count: Fixed(
                            1,
                        ),
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: with X=3, resolving deals 3 to the opponent's face and the caster draws 2 then
    /// discards 1 (net +1 in hand). Drives the `Discard` leaf end-to-end.
    #[test]
    fn traumatic_critique_deals_x_and_loots() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Target;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        // Discards the first offered card when asked to discard.
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

        // Caster library has two Forests to draw.
        let state = build_game(1, &[&[grp::FOREST, grp::FOREST], &[]]);
        let effect = state.card_db().get(TRAUMATIC_CRITIQUE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(DiscardAgent), Box::new(DiscardAgent)]);
        let p1 = e.state.player(PlayerId(1)).life;
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                x: Some(3),
                chosen_targets: vec![Target::Player(PlayerId(1))],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(1)).life, p1 - 3, "3 damage to the opponent's face");
        // Drew 2 then discarded 1 → net one card in hand (both draws came from the 2-card library).
        assert_eq!(e.state.players[0].hand.len(), 1, "drew two, discarded one → net one in hand");
        assert_eq!(e.state.players[0].graveyard.len(), 1, "the discarded card is in the graveyard");
    }
}
