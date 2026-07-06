//! Bitter Triumph — `{1}{B}` Instant (first printed LCI; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "As an additional cost to cast this spell, discard a card or pay 3 life. Destroy target
//! creature or planeswalker."
//!
//! **Fully implemented** — a **modal additional cost** (`AdditionalCost` with two options: discard a
//! card, or pay 3 life; the offer gate requires at least one payable, `choose_additional_options` picks
//! one at cast) + a single-target `Destroy` of a creature or planeswalker.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{AdditionalCost, Ability, Cost, CostComponent};
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const BITTER_TRIUMPH: u32 = 642;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Destroy {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Permanent(CardFilter::AnyOf(vec![
                CardFilter::HasCardType(CardType::Creature),
                CardFilter::HasCardType(CardType::Planeswalker),
            ])),
            min: 1,
            max: 1,
            distinct: true,
        }),
    };
    let mut def = spell(
        BITTER_TRIUMPH,
        "Bitter Triumph",
        CardType::Instant,
        Color::Black,
        mana_cost(1, &[(Color::Black, 1)]),
        effect,
    )
    .with_text("As an additional cost to cast this spell, discard a card or pay 3 life.\nDestroy target creature or planeswalker.");
    // Modal additional cost (CR 601.2b): discard a card OR pay 3 life.
    def.abilities.push(Ability::AdditionalCost(AdditionalCost {
        options: vec![
            Cost {
                mana: None,
                components: vec![CostComponent::Discard(SelectSpec {
                    zone: Zone::Hand,
                    filter: CardFilter::Any,
                    chooser: PlayerRef::Controller,
                    min: ValueExpr::Fixed(1),
                    max: ValueExpr::Fixed(1),
                })],
            },
            Cost { mana: None, components: vec![CostComponent::PayLife(ValueExpr::Fixed(3))] },
        ],
    }));
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Phase;
    use crate::cards::{build_game, grp};
    use crate::ids::PlayerId;
    use crate::priority::Engine;

    #[test]
    fn bitter_triumph_has_two_additional_cost_options() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BITTER_TRIUMPH).unwrap();
        assert!(def.fully_implemented);
        let ac = def.additional_costs();
        assert_eq!(ac.len(), 1, "one additional-cost clause");
        assert_eq!(ac[0].options.len(), 2, "modal: discard OR pay 3 life");
    }

    /// Picks the pay-life option (option 1) at the additional-cost choice, and the first target.
    #[derive(Clone)]
    struct PayLifeAndTarget;
    impl Agent for PayLifeAndTarget {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseOption { .. } => DecisionResponse::Index(1), // pay 3 life
                DecisionRequest::ChooseTargets { slots, .. } if !slots.is_empty() && !slots[0].legal.is_empty() => {
                    DecisionResponse::Pairs(vec![(0, 0)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Real cast paying the 3-life option (no card to discard needed): 3 life is paid at cast.
    #[test]
    fn real_cast_pays_three_life() {
        let mut state = build_game(1, &[&[], &[]]);
        let bitter = state.add_card(PlayerId(0), state.card_db().get(BITTER_TRIUMPH).unwrap().chars.clone(), Zone::Hand);
        for _ in 0..2 {
            state.add_card(PlayerId(0), state.card_db().get(grp::SWAMP).unwrap().chars.clone(), Zone::Battlefield); // {1}{B}
        }
        state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let life_before = state.players[0].life;
        let mut e = Engine::new(state, vec![Box::new(PayLifeAndTarget), Box::new(PayLifeAndTarget)]);
        e.cast_spell(PlayerId(0), bitter, CastVariant::Normal);
        assert_eq!(e.state.player(PlayerId(0)).life, life_before - 3, "paid 3 life as the additional cost");
    }
}
