//! Ad Nauseam — `{3}{B}{B}` Instant (first printed ALA / Shards of Alara; reprinted on the SOS
//! Mystical Archive `soa`).
//!
//! Oracle: "Reveal the top card of your library and put that card into your hand. You lose life equal
//! to its mana value. You may repeat this process any number of times."
//!
//! **Fully implemented** over the new `Effect::RevealTopLoseLifeMayRepeat` leaf: an imperative loop
//! that, before each iteration (so it may run zero or more times), reveals the top card, moves it to
//! hand, and makes you lose life equal to its mana value.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const AD_NAUSEAM: u32 = 643;

pub fn register(db: &mut CardDb) {
    db.insert(
        spell(
            AD_NAUSEAM,
            "Ad Nauseam",
            CardType::Instant,
            Color::Black,
            mana_cost(3, &[(Color::Black, 2)]),
            Effect::RevealTopLoseLifeMayRepeat,
        )
        .with_text("Reveal the top card of your library and put that card into your hand. You lose life equal to its mana value. You may repeat this process any number of times."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, ConfirmKind, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Zone;
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    /// Says "yes" to the first `n` reveal prompts, then "no".
    #[derive(Clone)]
    struct RevealN(std::cell::Cell<u32>);
    impl Agent for RevealN {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { kind: ConfirmKind::MayEffect } => {
                    let left = self.0.get();
                    if left > 0 {
                        self.0.set(left - 1);
                        DecisionResponse::Bool(true)
                    } else {
                        DecisionResponse::Bool(false)
                    }
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Reveal 3 cards (two Grizzly Bears = MV 2 each, one Hill Giant = MV 4): draw 3 to hand, lose
    /// 2+2+4 = 8 life, then stop.
    #[test]
    fn reveals_and_loses_life_by_mana_value() {
        let mut state = build_game(1, &[&[], &[]]);
        // Library bottom→top: [extra Forest] then Hill Giant, Bears, Bears on top (last = top).
        state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        state.add_card(PlayerId(0), state.card_db().get(grp::HILL_GIANT).unwrap().chars.clone(), Zone::Library); // MV 4
        state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Library); // MV 2
        state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Library); // MV 2 (top)
        let life_before = state.players[0].life;
        let effect = state.card_db().get(AD_NAUSEAM).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RevealN(std::cell::Cell::new(3))), Box::new(RevealN(std::cell::Cell::new(0)))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 3, "revealed 3 cards to hand");
        assert_eq!(e.state.player(PlayerId(0)).life, life_before - 8, "lost 2+2+4 = 8 life");
        assert_eq!(e.state.player(PlayerId(0)).library.len(), 1, "one Forest left");
    }
}
