//! Pursue the Past — `{R}{W}` Sorcery (first printed SOS).
//!
//! Oracle: "You gain 2 life. You may discard a card. If you do, draw two cards. / Flashback {2}{R}{W}"
//!
//! **Fully implemented** — gain 2 life, then an optional loot (`Optional{ IfYouDo{ discard a card,
//! draw two } }`), with `Ability::Flashback {2}{R}{W}`. Multicolored (R/W).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Ability;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const PURSUE_THE_PAST: u32 = 289;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(2) },
        Effect::Optional {
            prompt: "Discard a card to draw two?".to_string(),
            body: Box::new(Effect::IfYouDo {
                cost: Box::new(Effect::Discard { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) }),
                reward: Box::new(Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(2) }),
            }),
        },
    ]);
    let mut def = spell(PURSUE_THE_PAST, "Pursue the Past", CardType::Sorcery, Color::Red, mana_cost(0, &[(Color::Red, 1), (Color::White, 1)]), effect)
        .with_text("You gain 2 life. You may discard a card. If you do, draw two cards.\nFlashback {2}{R}{W}");
    def.chars.colors = vec![Color::Red, Color::White];
    def.abilities.push(Ability::Flashback { cost: mana_cost(2, &[(Color::Red, 1), (Color::White, 1)]) });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::ability::Ability;

    #[test]
    fn pursue_the_past_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PURSUE_THE_PAST).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Red, Color::White]);
        assert!(def.fully_implemented);
        assert!(def.abilities.iter().any(|a| matches!(a, Ability::Flashback { .. })));
    }

    #[test]
    fn pursue_the_past_gains_life_and_loots() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, ConfirmKind, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        // Agent: accept the "may", discard the first hand card.
        #[derive(Clone)] struct Looter;
        impl Agent for Looter {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::Confirm { kind: ConfirmKind::MayEffect } => DecisionResponse::Bool(true),
                    DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![0]),
                    _ => DecisionResponse::Pass,
                }
            }
        }
        // Hand has one card to discard; library has two to draw.
        let mut state = build_game(1, &[&[grp::FOREST, grp::ISLAND], &[]]);
        let filler = state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Hand);
        let _ = filler;
        let effect = state.card_db().get(PURSUE_THE_PAST).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Looter), Box::new(Looter)]);
        let (life0, hand0) = (e.state.player(PlayerId(0)).life, e.state.players[0].hand.len());
        e.resolve_effect(&effect, &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.player(PlayerId(0)).life, life0 + 2, "gained 2 life");
        // net hand: -1 discard +2 draw = +1
        assert_eq!(e.state.players[0].hand.len(), hand0 + 1, "discarded one, drew two");
    }
}
