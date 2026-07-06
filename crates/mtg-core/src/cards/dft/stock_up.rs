//! Stock Up — `{2}{U}` Sorcery (first printed DFT; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Look at the top five cards of your library. Put two of them into your hand and the rest
//! on the bottom of your library in any order."
//!
//! **Fully implemented** — a `LookAndPick`: look at the top five, take two to hand, the rest to the
//! bottom of the library.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::CardFilter;
use crate::effects::value::ValueExpr;
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const STOCK_UP: u32 = 618;

pub fn register(db: &mut CardDb) {
    let effect = Effect::LookAndPick {
        count: ValueExpr::Fixed(5),
        take: ValueExpr::Fixed(2),
        take_to: Zone::Hand,
        rest_to: Zone::Library,
        take_filter: CardFilter::Any,
    };
    db.insert(
        spell(
            STOCK_UP,
            "Stock Up",
            CardType::Sorcery,
            Color::Blue,
            mana_cost(2, &[(Color::Blue, 1)]),
            effect,
        )
        .with_text("Look at the top five cards of your library. Put two of them into your hand and the rest on the bottom of your library in any order."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    /// Takes the first two offered cards.
    #[derive(Clone)]
    struct TakeTwo;
    impl Agent for TakeTwo {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![0, 1]),
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn stock_up_draws_two_bottoms_the_rest() {
        let mut state = build_game(1, &[&[], &[]]);
        for _ in 0..7 {
            state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        }
        let effect = state.card_db().get(STOCK_UP).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(TakeTwo), Box::new(TakeTwo)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 2, "two cards taken to hand");
        assert_eq!(e.state.player(PlayerId(0)).library.len(), 5, "three bottomed, two untouched → 5 left");
    }
}
