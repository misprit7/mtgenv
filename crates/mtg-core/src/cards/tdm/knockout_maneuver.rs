//! Knockout Maneuver — `{2}{G}` Sorcery (first printed TDM; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Put a +1/+1 counter on target creature you control, then it deals damage equal to its
//! power to target creature an opponent controls."
//!
//! **Fully implemented** — slot 0 is your creature (the `SourcedDamage.source`), slot 1 an opponent's
//! creature. First a +1/+1 counter goes on slot 0 (`PutCounters{ChosenIndex(0)}`), then slot 0 deals
//! `PowerOfTarget(0)` (its now-boosted power) to slot 1 — the shipped `burrog_barrage` "it deals
//! damage equal to its power" idiom, with the pump applied first so the damage reads the boosted power.

use crate::basics::{CardType, Color, CounterKind, DamageKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const KNOCKOUT_MANEUVER: u32 = 629;

pub fn register(db: &mut CardDb) {
    let you_control = TargetSpec {
        kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
        min: 1,
        max: 1,
        distinct: true,
    };
    let opp_creature = TargetSpec {
        kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Opponent)),
        min: 1,
        max: 1,
        distinct: true,
    };
    let effect = Effect::Sequence(vec![
        Effect::PutCounters {
            what: EffectTarget::ChosenIndex(0),
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::Fixed(1),
        },
        Effect::SourcedDamage {
            source: EffectTarget::Target(you_control),
            to: EffectTarget::Target(opp_creature),
            amount: ValueExpr::PowerOfTarget(0),
            kind: DamageKind::Noncombat,
        },
    ]);
    db.insert(
        spell(
            KNOCKOUT_MANEUVER,
            "Knockout Maneuver",
            CardType::Sorcery,
            Color::Green,
            mana_cost(2, &[(Color::Green, 1)]),
            effect,
        )
        .with_text("Put a +1/+1 counter on target creature you control, then it deals damage equal to its power to target creature an opponent controls."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Target, Zone};
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

    /// Your 2/2 Bear gets a +1/+1 counter (→ 3/3), then deals 3 to an opponent's 2/2 Bear (marking
    /// lethal). Confirms the counter is applied before the power is read.
    #[test]
    fn counter_then_deals_boosted_power() {
        let mut state = build_game(1, &[&[], &[]]);
        let mine = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let theirs = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(KNOCKOUT_MANEUVER).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(mine), Target::Object(theirs)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let c = e.state.computed(mine);
        assert_eq!((c.power, c.toughness), (Some(3), Some(3)), "my creature got the +1/+1 counter");
        assert_eq!(e.state.object(theirs).damage_marked, 3, "dealt 3 (boosted power) to the opponent's creature");
    }
}
