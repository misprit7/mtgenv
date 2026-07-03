//! Embrace the Paradox — `{3}{G}{U}` Instant (first printed SOS).
//!
//! Oracle: "Draw three cards. You may put a land card from your hand onto the battlefield tapped."
//!
//! **Fully implemented** — `Draw 3` then a `Search` of the caster's **hand** for a land (`min 0` → the
//! optional "you may", `max 1`) put onto the battlefield tapped. `interpret_search` works over any
//! zone and only shuffles a *library*, so a hand → battlefield "search" is exactly this put-from-hand.
//! Multicolored (G/U).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const EMBRACE_THE_PARADOX: u32 = 315;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(3) },
        // "You may put a land card from your hand onto the battlefield tapped."
        Effect::Search {
            who: PlayerRef::Controller,
            zone: Zone::Hand,
            filter: CardFilter::HasCardType(CardType::Land),
            min: 0,
            max: 1,
            to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
            tapped: true,
        },
    ]);
    let mut def = spell(
        EMBRACE_THE_PARADOX,
        "Embrace the Paradox",
        CardType::Instant,
        Color::Green,
        mana_cost(3, &[(Color::Green, 1), (Color::Blue, 1)]),
        effect,
    )
    .with_text("Draw three cards. You may put a land card from your hand onto the battlefield tapped.");
    def.chars.colors = vec![Color::Green, Color::Blue];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embrace_the_paradox_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(EMBRACE_THE_PARADOX).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert_eq!(def.chars.colors, vec![Color::Green, Color::Blue]);
        assert!(def.fully_implemented);
    }

    /// Behaviour: draw three, then put a land from hand onto the battlefield tapped.
    #[test]
    fn embrace_draws_three_and_lands_a_land_tapped() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView, SelectReason};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        #[derive(Clone)]
        struct TakeFirstLand;
        impl Agent for TakeFirstLand {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    // The hand-search offers only lands; put the first onto the battlefield.
                    DecisionRequest::SelectCards { reason: SelectReason::Search, from, .. } if !from.is_empty() => {
                        DecisionResponse::Indices(vec![0])
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }
        // Library of three to draw; a Forest already in hand to put down.
        let lib = vec![grp::ISLAND, grp::MOUNTAIN, grp::GRIZZLY_BEARS];
        let mut state = build_game(1, &[&lib, &[]]);
        let land = state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Hand);
        let effect = state.card_db().get(EMBRACE_THE_PARADOX).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(TakeFirstLand), Box::new(TakeFirstLand)]);
        let hand0 = e.state.player(PlayerId(0)).hand.len();
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        // Drew 3 (hand +3), then put the Forest down (hand -1 back) → net +2, and the Forest is a
        // tapped battlefield permanent.
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand0 + 2, "drew 3, put 1 land down");
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&land), "the land entered the battlefield");
        assert!(e.state.object(land).status.tapped, "it entered tapped");
    }
}
