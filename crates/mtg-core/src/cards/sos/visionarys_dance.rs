//! Visionary's Dance — `{5}{U}{R}` Sorcery (first printed SOS).
//!
//! Oracle: "Create two 3/3 blue and red Elemental creature tokens with flying. / {2}, Discard this
//! card: Look at the top two cards of your library. Put one of them into your hand and the other into
//! your graveyard."
//!
//! **Fully implemented** — the sorcery makes two Elemental tokens; and a **hand-activated** loot
//! ability (`{2}` + discard this card → look 2, keep 1, bin 1) via the `DiscardSelfFromHand` path.
//! Multicolored (U/R).

use crate::basics::{CardType, Color, Zone};
use crate::cards::helpers::elemental_token;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const VISIONARYS_DANCE: u32 = 301;

pub fn register(db: &mut CardDb) {
    let mut def = spell(
        VISIONARYS_DANCE,
        "Visionary's Dance",
        CardType::Sorcery,
        Color::Blue,
        mana_cost(5, &[(Color::Blue, 1), (Color::Red, 1)]),
        Effect::CreateToken { spec: elemental_token(), count: ValueExpr::Fixed(2), controller: PlayerRef::Controller },
    )
    .with_text("Create two 3/3 blue and red Elemental creature tokens with flying.\n{2}, Discard this card: Look at the top two cards of your library. Put one of them into your hand and the other into your graveyard.");
    def.chars.colors = vec![Color::Blue, Color::Red];
    def.abilities.push(Ability::Activated {
        cost: Cost {
            mana: Some(mana_cost(2, &[])),
            components: vec![CostComponent::DiscardSelfFromHand],
        },
        effect: Effect::LookAndPick {
            count: ValueExpr::Fixed(2),
            take: ValueExpr::Fixed(1),
            take_to: Zone::Hand,
            rest_to: Zone::Graveyard,
        },
        timing: Timing::Instant,
        restriction: None,
        is_mana: false,
    });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visionarys_dance_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(VISIONARYS_DANCE).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Blue, Color::Red]);
        assert!(def.spell_effect().is_some(), "the token-making sorcery");
        assert!(def.abilities.iter().any(|a| matches!(a, Ability::Activated { cost, .. }
            if cost.components.iter().any(|c| matches!(c, CostComponent::DiscardSelfFromHand)))),
            "the hand-activated loot ability");
        assert!(def.fully_implemented);
    }

    /// From-hand cap: with the card in hand and `{2}` available, the discard-this ability is offered;
    /// activating it discards the card to the graveyard.
    #[test]
    fn visionarys_dance_hand_ability_offered_and_discards() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayableAction, PlayerView, SelectReason};
        use crate::basics::Phase;
        use crate::cards::{build_game, grp};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        #[derive(Clone)] struct KeepFirst;
        impl Agent for KeepFirst {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { reason: SelectReason::ScryStage, .. } => DecisionResponse::Indices(vec![0]),
                    _ => DecisionResponse::Pass,
                }
            }
        }
        let lib: Vec<u32> = std::iter::repeat(grp::FOREST).take(3).collect();
        let mut state = build_game(1, &[&lib, &[]]);
        state.active_player = PlayerId(0);
        let card = state.add_card(PlayerId(0), state.card_db().get(VISIONARYS_DANCE).unwrap().chars.clone(), Zone::Hand);
        // {2}: two Islands.
        for _ in 0..2 {
            state.add_card(PlayerId(0), state.card_db().get(grp::ISLAND).unwrap().chars.clone(), Zone::Battlefield);
        }
        let mut e = Engine::new(state, vec![Box::new(KeepFirst), Box::new(KeepFirst)]);
        e.state.phase = Phase::PrecombatMain;
        let act = e.legal_actions(PlayerId(0)).into_iter().find(|a| {
            matches!(a, PlayableAction::Activate { source, .. } if *source == card)
        });
        assert!(act.is_some(), "hand-activated ability offered");
        if let Some(PlayableAction::Activate { source, ability }) = act {
            e.activate_ability(PlayerId(0), source, ability);
        }
        assert!(e.state.players[0].graveyard.contains(&card), "the card was discarded as the cost");
        e.resolve_top();
        // Looked at 2, kept 1 → hand has that one card; library dropped by 2.
        assert!(e.state.players[0].hand.iter().any(|&o| o != card), "kept a card to hand");
    }
}
