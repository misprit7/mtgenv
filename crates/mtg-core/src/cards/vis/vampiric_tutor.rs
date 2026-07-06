//! Vampiric Tutor — `{B}` Instant (first printed VIS; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Search your library for a card, then shuffle and put that card on top. You lose 2 life."
//!
//! **Fully implemented** — a `Search` for any card whose destination is the **top of your library**
//! (the shuffle-then-place tutor path in `interpret_search`), then `LoseLife 2`.

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const VAMPIRIC_TUTOR: u32 = 634;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Search {
            who: PlayerRef::Controller,
            zone: Zone::Library,
            filter: CardFilter::Any,
            min: 0,
            max: 1,
            to: ZoneDest { zone: Zone::Library, pos: ZonePos::Top },
            tapped: false,
        },
        Effect::LoseLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(2) },
    ]);
    db.insert(
        spell(
            VAMPIRIC_TUTOR,
            "Vampiric Tutor",
            CardType::Instant,
            Color::Black,
            mana_cost(0, &[(Color::Black, 1)]),
            effect,
        )
        .with_text("Search your library for a card, then shuffle and put that card on top. You lose 2 life."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId, StackId};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::priority::Engine;

    /// Fetches a specific card (the Grizzly Bears) from the library.
    #[derive(Clone)]
    struct FetchBear(ObjId);
    impl Agent for FetchBear {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { from, .. } => {
                    let idx = from.iter().position(|&id| id == self.0).unwrap_or(0);
                    DecisionResponse::Indices(vec![idx as u32])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// The tutored card ends up on TOP of the library (drawn next) and the caster loses 2 life.
    #[test]
    fn tutors_a_card_to_the_top_and_loses_2() {
        use crate::basics::Zone;
        let mut state = build_game(1, &[&[], &[]]);
        // Five Forests + one Grizzly Bears in the library; we tutor the Bears.
        for _ in 0..5 {
            state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        }
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Library);
        let life_before = state.player(PlayerId(0)).life;
        let effect = state.card_db().get(VAMPIRIC_TUTOR).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(FetchBear(bear)), Box::new(FetchBear(bear))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).library.last().copied(), Some(bear), "tutored card is on top (the vec's tail)");
        assert_eq!(e.state.player(PlayerId(0)).life, life_before - 2, "lost 2 life");
    }
}
