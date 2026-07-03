//! Charging Strifeknight — `{2}{R}` Creature — Spirit Knight 3/3 (first printed SOS).
//!
//! Oracle: "Haste / {T}, Discard a card: Draw a card."
//!
//! **Fully implemented** — printed Haste + a `{T}, Discard a card:` activated loot (draw a card). The
//! discard is a `CostComponent::Discard` (a hand card the payer chooses); paying it is gated on having
//! a card to discard (an empty hand can't activate it).

use crate::basics::{Color, Zone};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, Keyword, Timing};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const CHARGING_STRIFEKNIGHT: u32 = 311;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        CHARGING_STRIFEKNIGHT,
        "Charging Strifeknight",
        &[CreatureType::Spirit, CreatureType::Knight],
        Color::Red,
        mana_cost(2, &[(Color::Red, 1)]),
        3,
        3,
        vec![Ability::Activated {
            cost: Cost {
                mana: None,
                components: vec![
                    CostComponent::TapSelf,
                    // "Discard a card" — one card of the payer's choice from hand.
                    CostComponent::Discard(SelectSpec {
                        zone: Zone::Hand,
                        filter: CardFilter::Any,
                        chooser: PlayerRef::Controller,
                        min: ValueExpr::Fixed(1),
                        max: ValueExpr::Fixed(1),
                    }),
                ],
            },
            effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
            timing: Timing::Instant,
            restriction: None,
            is_mana: false,
        }],
    );
    def.chars.keywords = vec![Keyword::Haste];
    def.text = "Haste\n{T}, Discard a card: Draw a card.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charging_strifeknight_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(CHARGING_STRIFEKNIGHT).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Haste]);
        assert!(def.fully_implemented);
        match &def.abilities[0] {
            Ability::Activated { cost, .. } => {
                assert!(cost.mana.is_none());
                assert!(matches!(cost.components[0], CostComponent::TapSelf));
                assert!(matches!(cost.components[1], CostComponent::Discard(_)));
            }
            o => panic!("expected Activated, got {o:?}"),
        }
    }

    /// Behaviour: the `{T}, Discard a card:` loot needs a card in hand (empty hand can't pay), and
    /// activating it taps the source, discards the chosen card, and draws one (hand net unchanged,
    /// graveyard +1, library −1).
    #[test]
    fn charging_strifeknight_loots() {
        use crate::agent::{AbilityRef, Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::cards::{build_game, grp};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        #[derive(Clone)]
        struct PassiveAgent;
        impl Agent for PassiveAgent {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }
        // Library of two, Strifeknight on the battlefield (untapped); the hand starts empty.
        let lib = vec![grp::FOREST, grp::MOUNTAIN];
        let mut state = build_game(1, &[&lib, &[]]);
        let knight = state.add_card(PlayerId(0), state.card_db().get(CHARGING_STRIFEKNIGHT).unwrap().chars.clone(), Zone::Battlefield);
        let cost = Cost {
            mana: None,
            components: vec![
                CostComponent::TapSelf,
                CostComponent::Discard(SelectSpec {
                    zone: Zone::Hand,
                    filter: CardFilter::Any,
                    chooser: PlayerRef::Controller,
                    min: ValueExpr::Fixed(1),
                    max: ValueExpr::Fixed(1),
                }),
            ],
        };
        let mut e = Engine::new(state, vec![Box::new(PassiveAgent), Box::new(PassiveAgent)]);
        // Empty hand → the discard cost is unpayable (CR 118.4 — can't pay a cost you can't meet).
        assert!(!e.can_pay_cost(PlayerId(0), knight, &cost), "an empty hand can't pay 'Discard a card'");
        // Give the payer a card to pitch.
        let pitch = e.state.add_card(PlayerId(0), e.state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Hand);
        assert!(e.can_pay_cost(PlayerId(0), knight, &cost), "a card in hand → the discard cost is payable");
        let (hand0, gy0, lib0) = (
            e.state.player(PlayerId(0)).hand.len(),
            e.state.player(PlayerId(0)).graveyard.len(),
            e.state.player(PlayerId(0)).library.len(),
        );
        e.activate_ability(PlayerId(0), knight, AbilityRef(0));
        e.resolve_top();
        assert!(e.state.object(knight).status.tapped, "the source is tapped ({{T}})");
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&pitch), "the pitched card went to the graveyard");
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand0, "discard one, draw one → hand size unchanged");
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), gy0 + 1, "the discarded card");
        assert_eq!(e.state.player(PlayerId(0)).library.len(), lib0 - 1, "drew one");
    }
}
