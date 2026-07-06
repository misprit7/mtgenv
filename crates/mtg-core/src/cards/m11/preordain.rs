//! Preordain — `{U}` Sorcery (first printed M11; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Scry 2, then draw a card."
//!
//! **Fully implemented** over the new `Effect::Scry` cap: `Scry 2` (look at the top two, put any
//! number on the bottom, rest on top) then `Draw 1`.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const PREORDAIN: u32 = 627;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Scry { count: ValueExpr::Fixed(2) },
        Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
    ]);
    db.insert(
        spell(
            PREORDAIN,
            "Preordain",
            CardType::Sorcery,
            Color::Blue,
            mana_cost(0, &[(Color::Blue, 1)]),
            effect,
        )
        .with_text("Scry 2, then draw a card."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView, SelectReason};
    use crate::basics::Zone;
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{ObjId, PlayerId, StackId};
    use crate::priority::Engine;

    /// At the scry stage, bottoms the FIRST shown card (the current top); draws otherwise.
    #[derive(Clone)]
    struct BottomTop;
    impl Agent for BottomTop {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { reason: SelectReason::ScryStage, .. } => DecisionResponse::Indices(vec![0]),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// P0's library (top-first) is [A, B, C, D]. Scry 2 sees [A, B]; the agent bottoms A. Then draw:
    /// the new top is B, so B is drawn. A ends up on the bottom.
    #[test]
    fn scry_2_bottoms_top_then_draws() {
        let mut state = build_game(1, &[&[], &[]]);
        // Insert in library-vec order (front = bottom, tail = top): push D, C, B, A so A is on top.
        let ids: Vec<ObjId> = ["D", "C", "B", "A"]
            .iter()
            .map(|_| state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library))
            .collect();
        // After add_card pushes, library = [D, C, B, A] with A on top (tail).
        let top_a = *ids.last().unwrap();
        let effect = state.card_db().get(PREORDAIN).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(BottomTop), Box::new(BottomTop)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 1, "drew one card");
        // A was bottomed, so it was NOT the drawn card; it sits at the bottom (front) of the library.
        assert!(!e.state.player(PlayerId(0)).hand.contains(&top_a), "the bottomed card A was not drawn");
        assert_eq!(e.state.player(PlayerId(0)).library.first().copied(), Some(top_a), "A is on the bottom");
    }
}
