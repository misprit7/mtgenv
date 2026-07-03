//! Send in the Pest — `{1}{B}` Sorcery (first printed SOS).
//!
//! Oracle: "Each opponent discards a card. You create a 1/1 black and green Pest creature token with
//! \"Whenever this token attacks, you gain 1 life.\""
//!
//! **Fully implemented** — each opponent discards, then create a Pest token whose attack-trigger comes
//! from the registered `PEST_TOKEN` def (the S11 token-with-ability path).

use crate::basics::{CardType, Color};
use crate::cards::helpers::pest_token;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const SEND_IN_THE_PEST: u32 = 290;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Discard { who: PlayerRef::EachOpponent, count: ValueExpr::Fixed(1) },
        Effect::CreateToken { spec: pest_token(), count: ValueExpr::Fixed(1), controller: PlayerRef::Controller, dynamic_counters: Vec::new() },
    ]);
    db.insert(
        spell(SEND_IN_THE_PEST, "Send in the Pest", CardType::Sorcery, Color::Black, mana_cost(1, &[(Color::Black, 1)]), effect)
            .with_text("Each opponent discards a card. You create a 1/1 black and green Pest creature token with \"Whenever this token attacks, you gain 1 life.\""),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Zone;
    use crate::cards::{build_game, grp};
    use crate::effects::ability::{Ability, EventPattern};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[derive(Clone)]
    struct DiscardFirst;
    impl Agent for DiscardFirst {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req { DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![0]), _ => DecisionResponse::Pass }
        }
    }

    /// S11 cap: the created Pest carries its attack-trigger ability (reachable via `def_of`), and the
    /// opponent discarded.
    #[test]
    fn send_in_the_pest_makes_an_ability_bearing_pest() {
        let mut state = build_game(1, &[&[], &[]]);
        // Give the opponent a card to discard.
        let _opp_card = state.add_card(PlayerId(1), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Hand);
        let effect = state.card_db().get(SEND_IN_THE_PEST).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(DiscardFirst), Box::new(DiscardFirst)]);
        let opp_hand0 = e.state.players[1].hand.len();
        e.resolve_effect(&effect, &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.players[1].hand.len(), opp_hand0 - 1, "each opponent discarded a card");
        let pest = *e.state.players[0].battlefield.iter().find(|&&o| e.state.object(o).chars.name == "Pest").expect("a Pest token exists");
        // The token's ability is supplied by the registered PEST_TOKEN def via def_of.
        let has_attack_trigger = e.state.def_of(pest).is_some_and(|d| {
            d.abilities.iter().any(|a| matches!(a, Ability::Triggered { event: EventPattern::SelfAttacks, .. }))
        });
        assert!(has_attack_trigger, "the Pest token carries its attack-trigger ability via its grp_id def");
    }
}
