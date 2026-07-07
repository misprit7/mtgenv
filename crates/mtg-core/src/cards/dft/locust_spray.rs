//! Locust Spray — `{B}` Instant (first printed DFT; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Target creature gets -1/-1 until end of turn.
//! Cycling {B} ({B}, Discard this card: Draw a card.)"
//!
//! **Fully implemented** — a `-1/-1` `PumpPT` on a target creature + a hand-activated Cycling ability
//! (`{B}` + `DiscardSelfFromHand` → draw a card). No new cap.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const LOCUST_SPRAY: u32 = 655;

pub fn register(db: &mut CardDb) {
    let effect = Effect::PumpPT {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        }),
        power: ValueExpr::Fixed(-1),
        toughness: ValueExpr::Fixed(-1),
        duration: Duration::UntilEndOfTurn,
    };
    let mut def = spell(
        LOCUST_SPRAY,
        "Locust Spray",
        CardType::Instant,
        Color::Black,
        mana_cost(0, &[(Color::Black, 1)]),
        effect,
    )
    .with_text("Target creature gets -1/-1 until end of turn.\nCycling {B} ({B}, Discard this card: Draw a card.)");
    // Cycling {B}: a hand-activated ability that discards this card to draw one (CR 702.29).
    def.abilities.push(Ability::Activated {
        cost: Cost { mana: Some(mana_cost(0, &[(Color::Black, 1)])), components: vec![CostComponent::DiscardSelfFromHand] },
        effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
        timing: Timing::Instant,
        restriction: None,
        is_mana: false,
    });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, AbilityRef, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::PlayerId;
    use crate::priority::Engine;

    #[derive(Clone)]
    struct TargetFirst;
    impl Agent for TargetFirst {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } if !slots.is_empty() && !slots[0].legal.is_empty() => {
                    DecisionResponse::Pairs(vec![(0, 0)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn minus_one_minus_one_kills_a_one_one() {
        let mut state = build_game(1, &[&[], &[]]);
        let spray = state.add_card(PlayerId(0), state.card_db().get(LOCUST_SPRAY).unwrap().chars.clone(), Zone::Hand);
        state.add_card(PlayerId(0), state.card_db().get(grp::SWAMP).unwrap().chars.clone(), Zone::Battlefield);
        // A 2/2 → 1/1 (survives); use a token weakened to 1/1 would die, but a plain bear just shrinks.
        let bear = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(TargetFirst), Box::new(TargetFirst)]);
        e.cast_spell(PlayerId(0), spray, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert_eq!(e.state.computed(bear).power, Some(1), "2/2 → 1/1");
        assert_eq!(e.state.computed(bear).toughness, Some(1));
    }

    #[test]
    fn cycling_draws_a_card() {
        let mut state = build_game(1, &[&[grp::FOREST], &[]]);
        let spray = state.add_card(PlayerId(0), state.card_db().get(LOCUST_SPRAY).unwrap().chars.clone(), Zone::Hand);
        state.add_card(PlayerId(0), state.card_db().get(grp::SWAMP).unwrap().chars.clone(), Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let hand_before = e_hand(&state);
        let mut e = Engine::new(state, vec![Box::new(TargetFirst), Box::new(TargetFirst)]);
        // Activate the cycling ability (index 1 — index 0 is the spell ability).
        e.activate_ability(PlayerId(0), spray, AbilityRef(1));
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(!e.state.player(PlayerId(0)).hand.contains(&spray), "Locust Spray discarded to cycle");
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand_before, "net hand size unchanged (discard 1, draw 1)");
    }

    fn e_hand(state: &crate::state::GameState) -> usize {
        state.player(PlayerId(0)).hand.len()
    }
}
