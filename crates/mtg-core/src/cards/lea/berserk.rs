//! Berserk — `{G}` Instant (first printed LEA; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Cast this spell only before the combat damage step. Target creature gains trample and gets
//! +X/+0 until end of turn, where X is its power. At the beginning of the next end step, destroy that
//! creature if it attacked this turn."
//!
//! **Fully implemented** (mechanics) — doubles the target's power (`PumpPT` with `PowerOfTarget`, the
//! Bulk Up idiom) + grants trample, and arms `Effect::DestroyAtEndStepIfAttacked` (a delayed end-step
//! trigger reading the new `Object::attacked_this_turn` flag).
//!
//! ⚠️ Documented divergence: the casting restriction "only before the combat damage step" is **not
//! enforced** — the engine offers Berserk at any instant-speed priority. Negligible in limited (it's
//! normally cast during combat anyway); a spell-level step-timing restriction has no engine seam yet.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const BERSERK: u32 = 657;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        // "Target creature ... gets +X/+0 until end of turn, where X is its power." (slot 0; the
        // `PowerOfTarget(0)` reads the pre-pump power, doubling it — the Bulk Up idiom.)
        Effect::PumpPT {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            power: ValueExpr::PowerOfTarget(0),
            toughness: ValueExpr::Fixed(0),
            duration: Duration::UntilEndOfTurn,
        },
        // "... gains trample ..."
        Effect::GrantKeyword {
            what: EffectTarget::ChosenIndex(0),
            keyword: Keyword::Trample,
            duration: Duration::UntilEndOfTurn,
        },
        // "At the beginning of the next end step, destroy that creature if it attacked this turn."
        Effect::DestroyAtEndStepIfAttacked { what: EffectTarget::ChosenIndex(0) },
    ]);
    let def = spell(
        BERSERK,
        "Berserk",
        CardType::Instant,
        Color::Green,
        mana_cost(0, &[(Color::Green, 1)]),
        effect,
    )
    .with_text("Cast this spell only before the combat damage step. Target creature gains trample and gets +X/+0 until end of turn, where X is its power. At the beginning of the next end step, destroy that creature if it attacked this turn.");
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
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

    fn setup(attacks: bool) -> (Engine, ObjId) {
        let mut state = build_game(1, &[&[], &[]]);
        let berserk = state.add_card(PlayerId(0), state.card_db().get(BERSERK).unwrap().chars.clone(), Zone::Hand);
        state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Battlefield);
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        state.objects.get_mut(&bear).unwrap().summoning_sick = false;
        state.active_player = PlayerId(0);
        state.phase = Phase::PostcombatMain;
        let mut e = Engine::new(state, vec![Box::new(TargetFirst), Box::new(TargetFirst)]);
        if attacks {
            e.declare_attackers_explicit(&[bear]);
        }
        e.cast_spell(PlayerId(0), berserk, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        (e, bear)
    }

    #[test]
    fn doubles_power_grants_trample() {
        let (e, bear) = setup(false);
        assert_eq!(e.state.computed(bear).power, Some(4), "2/2 → +2/+0 = 4/2");
        assert_eq!(e.state.computed(bear).toughness, Some(2));
        assert!(e.state.computed(bear).has_keyword(Keyword::Trample), "gains trample");
    }

    /// Attacked this turn → destroyed at the next end step.
    #[test]
    fn destroyed_at_end_step_if_it_attacked() {
        let (mut e, bear) = setup(true);
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&bear), "still alive before the end step");
        e.fire_end_step_delayed_triggers();
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&bear), "destroyed at end step (attacked)");
    }

    /// Did NOT attack → survives the end step (the delayed destroy is conditional).
    #[test]
    fn survives_end_step_if_it_did_not_attack() {
        let (mut e, bear) = setup(false);
        e.fire_end_step_delayed_triggers();
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&bear), "survives (did not attack)");
    }
}
