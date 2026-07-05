//! Orysa, Tide Choreographer — `{4}{U}` Legendary Creature — Merfolk Bard 2/2 (first printed SOS).
//!
//! Oracle: "This spell costs {3} less to cast if creatures you control have total toughness 10 or
//! greater. When Orysa enters, draw two cards."
//!
//! **Fully implemented** — exercises the new **S12 cost-modification pipeline** (CR 601.2f / 118):
//! an `Ability::CostReduction { amount: Generic(3), condition: ValueAtLeast(TotalToughness{creatures
//! you control}, 10) }`, applied by `effective_cast_cost` at BOTH the offer gate (so Orysa becomes
//! affordable when the board is wide enough) and at payment. Plus a trivial ETB `Draw 2`.

use crate::basics::{CardType, Color};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, CostReductionAmount, CostReductionCondition, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const ORYSA_TIDE_CHOREOGRAPHER: u32 = 358;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        ORYSA_TIDE_CHOREOGRAPHER,
        "Orysa, Tide Choreographer",
        &[CreatureType::Merfolk, CreatureType::Bard],
        Color::Blue,
        mana_cost(4, &[(Color::Blue, 1)]),
        2,
        2,
        vec![
            // "This spell costs {3} less to cast if creatures you control have total toughness 10+."
            Ability::CostReduction {
                amount: CostReductionAmount::Generic(3),
                condition: CostReductionCondition::State(Condition::ValueAtLeast(
                    ValueExpr::TotalToughness {
                        filter: CardFilter::HasCardType(CardType::Creature),
                        controller: Some(PlayerRef::Controller),
                    },
                    ValueExpr::Fixed(10),
                )),
            },
            // "When Orysa enters, draw two cards."
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(2) },
            },
        ],
    )
    .with_text(
        "This spell costs {3} less to cast if creatures you control have total toughness 10 or greater.\nWhen Orysa enters, draw two cards.",
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{PlayableAction, RandomAgent};
    use crate::basics::{Phase, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::PlayerId;
    use crate::priority::Engine;

    #[test]
    fn orysa_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ORYSA_TIDE_CHOREOGRAPHER).unwrap();
        assert!(def.fully_implemented);
        assert!(def.chars.supertypes.contains(&Supertype::Legendary));
        assert!(matches!(&def.abilities[0], Ability::CostReduction { .. }));
    }

    /// Populate P0's board with total toughness ≥ 10, then assert `effective_cast_cost` drops the
    /// generic by 3 ({4}{U} → {1}{U}); below the threshold it stays {4}{U}.
    #[test]
    fn cost_reduces_when_board_toughness_is_high() {
        let mut state = build_game(1, &[&[], &[]]);
        let orysa = state.add_card(
            PlayerId(0),
            state.card_db().get(ORYSA_TIDE_CHOREOGRAPHER).unwrap().chars.clone(),
            Zone::Hand,
        );
        let base = state.object(orysa).chars.mana_cost.clone().unwrap();
        let e0 = {
            let e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
            e.effective_cast_cost(PlayerId(0), orysa, &base, crate::priority::TargetCtx::Optimistic)
        };
        assert_eq!(e0.generic, 4, "no reduction with an empty board");

        // Now five 2/2 Grizzly Bears = total toughness 10.
        let mut state = build_game(1, &[&[], &[]]);
        let orysa = state.add_card(
            PlayerId(0),
            state.card_db().get(ORYSA_TIDE_CHOREOGRAPHER).unwrap().chars.clone(),
            Zone::Hand,
        );
        for _ in 0..5 {
            let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), bears, Zone::Battlefield);
        }
        let e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let reduced = e.effective_cast_cost(PlayerId(0), orysa, &base, crate::priority::TargetCtx::Optimistic);
        assert_eq!(reduced.generic, 1, "toughness 10 → {{3}} off → {{1}}{{U}}");
        assert_eq!(reduced.colored.get(&Color::Blue), Some(&1), "coloured pip untouched");
    }

    /// Real-path offer gate: with only {1}{U} of mana but a board of total toughness ≥ 10, the
    /// {4}{U} Orysa is castable (affordable only via the reduction). Remove the board → not offered.
    #[test]
    fn offered_only_when_reduction_makes_it_affordable() {
        let offered_with_board = |wide: bool| {
            let mut state = build_game(1, &[&[], &[]]);
            state.add_card(
                PlayerId(0),
                state.card_db().get(ORYSA_TIDE_CHOREOGRAPHER).unwrap().chars.clone(),
                Zone::Hand,
            );
            // Exactly {1}{U} available: an Island + a Plains.
            let island = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            let plains = state.card_db().get(grp::PLAINS).unwrap().chars.clone();
            state.add_card(PlayerId(0), island, Zone::Battlefield);
            state.add_card(PlayerId(0), plains, Zone::Battlefield);
            if wide {
                for _ in 0..5 {
                    let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
                    state.add_card(PlayerId(0), bears, Zone::Battlefield);
                }
            }
            state.active_player = PlayerId(0);
            state.phase = Phase::PrecombatMain;
            let e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
            e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::Cast { .. }))
        };
        assert!(offered_with_board(true), "castable via the {{3}} reduction");
        assert!(!offered_with_board(false), "unaffordable at full {{4}}{{U}} → not offered");
    }
}
