//! Rabid Attack — `{1}{B}` Instant (first printed SOS).
//!
//! Oracle: "Until end of turn, any number of target creatures you control each get +1/+0 and gain
//! 'When this creature dies, draw a card.'"
//!
//! **Fully implemented** — the lander for **granting a triggered ability until end of turn** (CR
//! 613.1f). A `ForEachTarget` over "any number of target creatures you control" gives each a `+1/+0`
//! `PumpPT` and an `Effect::GrantAbility` pointing at the reserved template
//! [`grp::GRANT_DIES_DRAW`] ("When this creature dies, draw a card"). The granted trigger fires from
//! the creature (via the `queue_self_triggers` granted-ability scan) and expires with the continuous
//! effect at end of turn — so a creature that dies AFTER the turn ends does not draw.

use crate::basics::{CardType, Color};
use crate::cards::{grp, mana_cost, spell, CardDb};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const RABID_ATTACK: u32 = 420;

pub fn register(db: &mut CardDb) {
    let effect = Effect::ForEachTarget {
        // "any number of target creatures you control"
        slot: TargetSpec {
            kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
            min: 0,
            max: 99,
            distinct: true,
        },
        body: Box::new(Effect::Sequence(vec![
            Effect::PumpPT {
                what: EffectTarget::Each,
                power: ValueExpr::Fixed(1),
                toughness: ValueExpr::Fixed(0),
                duration: Duration::UntilEndOfTurn,
            },
            Effect::GrantAbility {
                what: EffectTarget::Each,
                template_grp: grp::GRANT_DIES_DRAW,
                duration: Duration::UntilEndOfTurn,
            },
        ])),
    };
    db.insert(
        spell(RABID_ATTACK, "Rabid Attack", CardType::Instant, Color::Black, mana_cost(1, &[(Color::Black, 1)]), effect)
            .with_text("Until end of turn, any number of target creatures you control each get +1/+0 and gain \"When this creature dies, draw a card.\""),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView, RandomAgent};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    /// An agent that targets `pick` at ChooseTargets (single slot), else passes.
    struct PickAgent(ObjId);
    impl Agent for PickAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let idx = slots[0].legal.iter().position(|t| *t == Target::Object(self.0)).unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Set up P0 with Rabid Attack + {1}{B}, a 2/2 Grizzly Bears, and `lib` library cards. Returns
    /// `(engine, rabid, bears)`.
    fn setup(lib: usize) -> (Engine, ObjId, ObjId) {
        let mut state = build_game(1, &[&[], &[]]);
        let rabid = state.add_card(
            PlayerId(0),
            state.card_db().get(RABID_ATTACK).unwrap().chars.clone(),
            Zone::Hand,
        );
        for _ in 0..2 {
            let c = state.card_db().get(grp::SWAMP).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        for _ in 0..lib {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(PickAgent(bears)), Box::new(RandomAgent::new(1))]);
        (e, rabid, bears)
    }

    /// Kill `obj` via lethal damage + the SBA/agenda, then resolve whatever trigger it put on the stack.
    fn kill_and_settle(e: &mut Engine, obj: ObjId) {
        let t = e.state.computed(obj).toughness.unwrap_or(1).max(1) as u32;
        e.state.objects.get_mut(&obj).unwrap().damage_marked = t;
        e.state.mark_chars_dirty();
        e.run_agenda(); // SBA death → dies-trigger collected + put on the stack
        while !e.state.stack.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
    }

    /// Grant → +1/+0 applied, then the creature dies → the granted "draw a card" fires.
    #[test]
    fn granted_dies_trigger_draws_when_it_dies() {
        let (mut e, rabid, bears) = setup(3);
        e.cast_spell(PlayerId(0), rabid, CastVariant::Normal);
        e.resolve_top();
        assert_eq!(e.state.computed(bears).power, Some(3), "+1/+0 applied (2→3 power)");
        let hand_before = e.state.player(PlayerId(0)).hand.len();

        kill_and_settle(&mut e, bears);
        assert!(!e.state.player(PlayerId(0)).battlefield.contains(&bears), "the bears died");
        assert_eq!(
            e.state.player(PlayerId(0)).hand.len(),
            hand_before + 1,
            "the granted 'when this dies, draw a card' fired"
        );
    }

    /// The grant is until end of turn: after the turn's continuous effects expire, a death does NOT draw.
    #[test]
    fn granted_trigger_expires_at_end_of_turn() {
        let (mut e, rabid, bears) = setup(3);
        e.cast_spell(PlayerId(0), rabid, CastVariant::Normal);
        e.resolve_top();
        // End-of-turn cleanup drops the "until end of turn" grant (and the +1/+0).
        e.state.end_of_turn_continuous_cleanup();
        e.state.mark_chars_dirty();
        assert_eq!(e.state.computed(bears).power, Some(2), "back to base 2 power after EOT");
        let hand_before = e.state.player(PlayerId(0)).hand.len();

        kill_and_settle(&mut e, bears);
        assert!(!e.state.player(PlayerId(0)).battlefield.contains(&bears), "the bears died");
        assert_eq!(
            e.state.player(PlayerId(0)).hand.len(),
            hand_before,
            "post-EOT death does NOT draw (the grant expired)"
        );
    }
}
