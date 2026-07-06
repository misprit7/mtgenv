//! Dismember — `{1}{B/P}{B/P}` Instant (first printed NPH; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Target creature gets -5/-5 until end of turn." (`{B/P}` can be paid with either `{B}` or 2 life.)
//!
//! **Fully implemented** over the new **phyrexian pip** class: the cost carries two `{B/P}` pips (each
//! payable by one black mana OR 2 life; mana value 3). Its effect is a single-target `PumpPT` of -5/-5
//! until end of turn.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost_phyrexian, spell, CardDb};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const DISMEMBER: u32 = 641;

pub fn register(db: &mut CardDb) {
    let effect = Effect::PumpPT {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        }),
        power: ValueExpr::Fixed(-5),
        toughness: ValueExpr::Fixed(-5),
        duration: Duration::UntilEndOfTurn,
    };
    db.insert(
        spell(
            DISMEMBER,
            "Dismember",
            CardType::Instant,
            Color::Black,
            mana_cost_phyrexian(1, &[], &[Color::Black, Color::Black]),
            effect,
        )
        .with_text("Target creature gets -5/-5 until end of turn. ({B/P} can be paid with either {B} or 2 life.)"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Target, Zone};
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

    #[test]
    fn dismember_mana_value_is_three() {
        let mut db = CardDb::default();
        register(&mut db);
        assert_eq!(db.get(DISMEMBER).unwrap().chars.mana_value(), 3, "{{1}}{{B/P}}{{B/P}} = MV 3");
    }

    /// Resolution: -5/-5 kills a 3/3.
    #[test]
    fn dismember_shrinks_a_creature() {
        let mut state = build_game(1, &[&[], &[]]);
        let giant = state.add_card(PlayerId(1), state.card_db().get(grp::HILL_GIANT).unwrap().chars.clone(), Zone::Battlefield); // 3/3
        let effect = state.card_db().get(DISMEMBER).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(giant)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let c = e.state.computed(giant);
        assert_eq!((c.power, c.toughness), (Some(-2), Some(-2)), "3/3 gets -5/-5");
    }

    /// Picks the first legal target for the single slot.
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

    /// Real cast path with NO black mana: the two {B/P} are paid with 4 life (via auto-pay), only the
    /// {1} is tapped from a Forest, no mana leaks, and the target shrinks by -5/-5.
    #[test]
    fn real_cast_pays_the_phyrexian_pips_with_life() {
        let mut state = build_game(1, &[&[], &[]]);
        let dismember = state.add_card(PlayerId(0), state.card_db().get(DISMEMBER).unwrap().chars.clone(), Zone::Hand);
        // One Forest pays the {1}; no black source, so the phyrexian pips cost life.
        state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Battlefield);
        let victim = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let life_before = state.players[0].life;
        let mut e = Engine::new(state, vec![Box::new(TargetFirst), Box::new(TargetFirst)]);
        e.cast_spell(PlayerId(0), dismember, CastVariant::Normal);
        assert_eq!(e.state.player(PlayerId(0)).life, life_before - 4, "two {{B/P}} paid with 4 life");
        assert!(
            e.state.player(PlayerId(0)).mana_pool.amounts.values().all(|&v| v == 0),
            "no floating mana leaked"
        );
        // Resolve the spell → the bears (2/2) get -5/-5 and die to SBAs.
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(!e.state.player(PlayerId(1)).battlefield.contains(&victim), "the -5/-5'd creature died");
    }
}
