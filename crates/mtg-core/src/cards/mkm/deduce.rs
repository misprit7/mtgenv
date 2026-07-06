//! Deduce — `{1}{U}` Instant (first printed MKM; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Draw a card. Investigate. (Create a Clue token. It's an artifact with \"{2}, Sacrifice
//! this token: Draw a card.\")"
//!
//! **Fully implemented** — `Draw 1` then Investigate = a Clue token (the shared `helpers::clue_token`,
//! whose `{2}, Sacrifice: Draw` ability comes from the registered `grp::CLUE_TOKEN` def).

use crate::basics::{CardType, Color};
use crate::cards::{helpers, mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const DEDUCE: u32 = 631;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
        Effect::CreateToken {
            spec: helpers::clue_token(),
            count: ValueExpr::Fixed(1),
            controller: PlayerRef::Controller,
            dynamic_counters: vec![],
        },
    ]);
    db.insert(
        spell(
            DEDUCE,
            "Deduce",
            CardType::Instant,
            Color::Blue,
            mana_cost(1, &[(Color::Blue, 1)]),
            effect,
        )
        .with_text("Draw a card. Investigate. (Create a Clue token. It's an artifact with \"{2}, Sacrifice this token: Draw a card.\")"),
    );
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

    #[derive(Clone)]
    struct Passive;
    impl Agent for Passive {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    /// Resolving draws a card and creates a Clue token (which carries the registered draw ability).
    #[test]
    fn deduce_draws_and_makes_a_clue() {
        let mut state = build_game(1, &[&[], &[]]);
        for _ in 0..3 {
            state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        }
        let effect = state.card_db().get(DEDUCE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 1, "drew a card");
        let clue = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .find(|&&id| e.state.object(id).chars.grp_id == grp::CLUE_TOKEN)
            .copied();
        assert!(clue.is_some(), "a Clue token was created");
        // The Clue's def carries its activated "{2}, Sacrifice: Draw a card" ability.
        let card_db = e.state.card_db();
        let def = card_db.get(grp::CLUE_TOKEN).unwrap();
        assert!(matches!(def.abilities[0], crate::effects::ability::Ability::Activated { is_mana: false, .. }));
    }
}
