//! Lumaret's Favor — `{1}{G}` Instant.
//!
//! Oracle: "Infusion — When you cast this spell, copy it if you gained life this turn. You may choose
//! new targets for the copy. Target creature gets +2/+4 until end of turn."
//!
//! **Fully implemented** — a copy-self Infusion instant over the SelfCast + CopySpellOnStack caps:
//! the main effect is `PumpPT{ target creature, +2/+4, EOT }`; the Infusion clause is a
//! `Triggered{ SelfCast, if GainedLifeThisTurn → CopySpellOnStack{ Triggering, 1, new targets } }`.
//! When cast, if you gained life this turn the self-cast trigger fires (above the still-on-stack
//! spell) and copies it, so the copy resolves first — a second +2/+4 (its own re-chosen target).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const LUMARETS_FAVOR: u32 = 427;

pub fn register(db: &mut CardDb) {
    let mut def = spell(
        LUMARETS_FAVOR,
        "Lumaret's Favor",
        CardType::Instant,
        Color::Green,
        mana_cost(1, &[(Color::Green, 1)]),
        Effect::PumpPT {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            power: ValueExpr::Fixed(2),
            toughness: ValueExpr::Fixed(4),
            duration: Duration::UntilEndOfTurn,
        },
    );
    // Infusion — "When you cast this spell, copy it if you gained life this turn." A SelfCast trigger
    // gated on the Infusion condition; the copy re-offers new targets (707.10c).
    def.abilities.push(Ability::Triggered {
        event: EventPattern::SelfCast,
        condition: Some(Condition::GainedLifeThisTurn { who: PlayerRef::Controller }),
        intervening_if: false,
        effect: Effect::CopySpellOnStack {
            what: EffectTarget::Triggering,
            count: ValueExpr::Fixed(1),
            choose_new_targets: true,
        },
    });
    def.text = "Infusion — When you cast this spell, copy it if you gained life this turn. You may choose new targets for the copy.\nTarget creature gets +2/+4 until end of turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::{grp, starter_db};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn lumarets_favor_shape() {
        let db = db_with_card();
        let def = db.get(LUMARETS_FAVOR).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert!(def.fully_implemented);
        assert!(matches!(def.abilities[0], Ability::Spell { .. }));
        assert!(matches!(
            def.abilities[1],
            Ability::Triggered { event: EventPattern::SelfCast, .. }
        ));
    }

    /// Targets the named creature for both the original pump and (if offered) the copy's reselection.
    #[derive(Clone)]
    struct PumpAgent {
        target: ObjId,
    }
    impl Agent for PumpAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let i = slots[0]
                        .legal
                        .iter()
                        .position(|t| matches!(t, Target::Object(o) if *o == self.target))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, i as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn drive(e: &mut Engine) {
        loop {
            e.run_agenda();
            if e.state.stack.items.is_empty() {
                break;
            }
            e.resolve_top();
        }
    }

    /// Build P0 with Lumaret's Favor in hand + a Forest to cast it + a Grizzly Bears (2/2) target.
    /// `gained_life` seeds `life_gained_this_turn` so the Infusion clause is (in)active.
    fn setup(gained_life: bool) -> (Engine, ObjId, ObjId) {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let favor = {
            let c = state.card_db().get(LUMARETS_FAVOR).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        for _ in 0..2 {
            let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), f, Zone::Battlefield);
        }
        if gained_life {
            state.player_mut(PlayerId(0)).life_gained_this_turn = 3;
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(PumpAgent { target: bears }), Box::new(PumpAgent { target: bears })]);
        (e, favor, bears)
    }

    /// Infusion active (gained life): the spell is copied → the Bears gets +2/+4 TWICE = +4/+8 (a 6/10).
    #[test]
    fn infusion_copies_when_you_gained_life() {
        let (mut e, favor, bears) = setup(true);
        e.cast_spell(PlayerId(0), favor, CastVariant::Normal);
        drive(&mut e);
        let b = e.state.computed(bears);
        assert_eq!((b.power, b.toughness), (Some(6), Some(10)), "+2/+4 applied twice (copy + original)");
    }

    /// Infusion inactive (no life gained): no copy → the Bears gets +2/+4 once = a 4/6.
    #[test]
    fn no_copy_when_you_did_not_gain_life() {
        let (mut e, favor, bears) = setup(false);
        e.cast_spell(PlayerId(0), favor, CastVariant::Normal);
        drive(&mut e);
        let b = e.state.computed(bears);
        assert_eq!((b.power, b.toughness), (Some(4), Some(6)), "+2/+4 once (no copy)");
    }
}
