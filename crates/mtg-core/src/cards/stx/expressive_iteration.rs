//! Expressive Iteration — `{U}{R}` Sorcery (first printed STX; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Look at the top three cards of your library. Put one of them into your hand, put one of
//! them on the bottom of your library, and exile one of them. You may play the exiled card this turn."
//!
//! **Fully implemented** over the new `Effect::LookDistribute` leaf: look at the top three, put one in
//! hand, exile one (playable this turn), and bottom the remaining one.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, PlayWindow};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const EXPRESSIVE_ITERATION: u32 = 644;

pub fn register(db: &mut CardDb) {
    let effect = Effect::LookDistribute {
        count: ValueExpr::Fixed(3),
        to_hand: 1,
        to_exile_play: 1,
        window: PlayWindow::ThisTurn,
    };
    let mut def = spell(
        EXPRESSIVE_ITERATION,
        "Expressive Iteration",
        CardType::Sorcery,
        Color::Blue,
        mana_cost(0, &[(Color::Blue, 1), (Color::Red, 1)]),
        effect,
    )
    .with_text("Look at the top three cards of your library. Put one of them into your hand, put one of them on the bottom of your library, and exile one of them. You may play the exiled card this turn.");
    def.chars.colors = vec![Color::Blue, Color::Red];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Zone;
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    /// Always takes the first offered card at each selection.
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

    /// Top 3 → 1 to hand, 1 exiled (playable), 1 bottomed. Library of 5 → 1 to hand, 1 exiled, so
    /// 5-2 = 3 remain in the library (2 untouched + 1 bottomed).
    #[test]
    fn distributes_hand_exile_bottom() {
        let mut state = build_game(1, &[&[], &[]]);
        for _ in 0..5 {
            state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        }
        let effect = state.card_db().get(EXPRESSIVE_ITERATION).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(TakeFirst), Box::new(TakeFirst)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 1, "one card to hand");
        assert_eq!(e.state.player(PlayerId(0)).exile.len(), 1, "one card exiled to play");
        assert_eq!(e.state.player(PlayerId(0)).library.len(), 3, "one bottomed + two untouched");
        // The exiled card is playable this turn.
        let exiled = e.state.player(PlayerId(0)).exile[0];
        assert!(e.state.object(exiled).castable_from_exile, "exiled card is playable");
    }
}
