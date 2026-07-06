//! Sleight of Hand — `{U}` Sorcery (first printed P02 / Portal Second Age; reprinted on the SOS
//! Mystical Archive `soa`).
//!
//! Oracle: "Look at the top two cards of your library. Put one of them into your hand and the other
//! on the bottom of your library."
//!
//! **Fully implemented** — a `LookAndPick`: look at the top two, take one to hand, the rest to the
//! bottom of the library.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::CardFilter;
use crate::effects::value::ValueExpr;
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const SLEIGHT_OF_HAND: u32 = 617;

pub fn register(db: &mut CardDb) {
    let effect = Effect::LookAndPick {
        count: ValueExpr::Fixed(2),
        take: ValueExpr::Fixed(1),
        take_to: Zone::Hand,
        rest_to: Zone::Library,
        take_filter: CardFilter::Any,
    };
    db.insert(
        spell(
            SLEIGHT_OF_HAND,
            "Sleight of Hand",
            CardType::Sorcery,
            Color::Blue,
            mana_cost(0, &[(Color::Blue, 1)]),
            effect,
        )
        .with_text("Look at the top two cards of your library. Put one of them into your hand and the other on the bottom of your library."),
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

    /// Takes the first offered card at the look-and-pick.
    #[derive(Clone)]
    struct TakeFirst;
    impl Agent for TakeFirst {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![0]),
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn sleight_of_hand_draws_one_bottoms_one() {
        let mut state = build_game(1, &[&[], &[]]);
        for _ in 0..4 {
            state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        }
        let effect = state.card_db().get(SLEIGHT_OF_HAND).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(TakeFirst), Box::new(TakeFirst)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 1, "one card taken to hand");
        assert_eq!(e.state.player(PlayerId(0)).library.len(), 3, "one bottomed, two untouched → 3 left");
    }
}
