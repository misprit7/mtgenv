//! Stargaze — `{X}{B}{B}` Sorcery (first printed BLB; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Look at twice X cards from the top of your library. Put X cards from among them into your
//! hand and the rest into your graveyard. You lose X life."
//!
//! **Fully implemented** — `LookAndPick { count: 2X, take: X, take_to: Hand, rest_to: Graveyard }`
//! then `LoseLife X`. (`XTimes(2)` = 2 × the X chosen when casting.)

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const STARGAZE: u32 = 628;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::LookAndPick {
            count: ValueExpr::XTimes(2),
            take: ValueExpr::X,
            take_to: Zone::Hand,
            rest_to: Zone::Graveyard,
            take_filter: CardFilter::Any,
        },
        Effect::LoseLife { who: PlayerRef::Controller, amount: ValueExpr::X },
    ]);
    db.insert(
        spell(
            STARGAZE,
            "Stargaze",
            CardType::Sorcery,
            Color::Black,
            mana_cost(0, &[(Color::Black, 2)]),
            effect,
        )
        .with_text("Look at twice X cards from the top of your library. Put X cards from among them into your hand and the rest into your graveyard. You lose X life."),
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

    /// Takes the first X offered cards at the look-and-pick.
    #[derive(Clone)]
    struct TakeFirstX(u32);
    impl Agent for TakeFirstX {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { .. } => DecisionResponse::Indices((0..self.0).collect()),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// X=2: look at 4 cards, take 2 to hand, 2 to graveyard, lose 2 life.
    #[test]
    fn stargaze_x2() {
        let mut state = build_game(1, &[&[], &[]]);
        for _ in 0..6 {
            state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        }
        let life_before = state.player(PlayerId(0)).life;
        let effect = state.card_db().get(STARGAZE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(TakeFirstX(2)), Box::new(TakeFirstX(2))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), x: Some(2), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 2, "took X=2 to hand");
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 2, "rest of the 2X=4 to graveyard");
        assert_eq!(e.state.player(PlayerId(0)).life, life_before - 2, "lost X=2 life");
    }
}
