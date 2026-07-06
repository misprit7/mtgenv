//! Glimpse of Nature — `{G}` Sorcery (first printed CHK / Champions of Kamigawa; reprinted on the SOS
//! Mystical Archive `soa`).
//!
//! Oracle: "Whenever you cast a creature spell this turn, draw a card."
//!
//! **Fully implemented** over the new `Effect::WheneverYouCastThisTurn` leaf: arms a recurring
//! YouCastSpell delayed trigger (creature spells) whose reaction draws a card. It fires for every
//! creature spell you cast this turn and expires at the next turn's start.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const GLIMPSE_OF_NATURE: u32 = 645;

pub fn register(db: &mut CardDb) {
    let effect = Effect::WheneverYouCastThisTurn {
        filter: CardFilter::HasCardType(CardType::Creature),
        effect: Box::new(Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) }),
    };
    db.insert(
        spell(
            GLIMPSE_OF_NATURE,
            "Glimpse of Nature",
            CardType::Sorcery,
            Color::Green,
            mana_cost(0, &[(Color::Green, 1)]),
            effect,
        )
        .with_text("Whenever you cast a creature spell this turn, draw a card."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[derive(Clone)]
    struct Passive;
    impl Agent for Passive {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    /// After Glimpse resolves, casting two creature spells this turn draws two cards (recurring).
    #[test]
    fn draws_per_creature_cast_this_turn() {
        let mut state = build_game(1, &[&[], &[]]);
        // Library to draw from.
        for _ in 0..4 {
            state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        }
        // Two Grizzly Bears in hand to cast, and forests to pay for them.
        let bear1 = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Hand);
        let bear2 = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Hand);
        for _ in 0..6 {
            state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        // Arm Glimpse (resolve its effect).
        let effect = state.card_db().get(GLIMPSE_OF_NATURE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let hand_before = e.state.player(PlayerId(0)).hand.len(); // the two bears (2)
        // Cast the first creature — the Glimpse trigger draws a card.
        e.cast_spell(PlayerId(0), bear1, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        // Cast the second creature — draws again.
        e.cast_spell(PlayerId(0), bear2, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        // Hand: started with 2 bears; cast both (−2), drew 2 (Glimpse) → net back to hand_before − 2 + 2.
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand_before - 2 + 2, "drew a card per creature cast");
        // Both bears resolved onto the battlefield.
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&bear1) && e.state.player(PlayerId(0)).battlefield.contains(&bear2));
    }
}
