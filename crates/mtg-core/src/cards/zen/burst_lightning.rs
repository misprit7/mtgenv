//! Burst Lightning — `{R}` Instant (first printed ZEN; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Kicker {4} (You may pay an additional {4} as you cast this spell.)
//! Burst Lightning deals 2 damage to any target. If this spell was kicked, it deals 4 damage instead."
//!
//! **Fully implemented** — the kicker subsystem (`Ability::Kicker` + `Object.kicked` +
//! `ValueExpr::IfKicked`): the cast pipeline offers the optional `{4}` and records `kicked`; the damage
//! amount reads `IfKicked{ 4, 2 }`.

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Ability;
use crate::effects::target::{TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const BURST_LIGHTNING: u32 = 656;

pub fn register(db: &mut CardDb) {
    let effect = Effect::DealDamage {
        amount: ValueExpr::IfKicked {
            yes: Box::new(ValueExpr::Fixed(4)),
            no: Box::new(ValueExpr::Fixed(2)),
        },
        to: EffectTarget::Target(TargetSpec { kind: TargetKind::Any, min: 1, max: 1, distinct: true }),
        kind: DamageKind::Noncombat,
    };
    let mut def = spell(
        BURST_LIGHTNING,
        "Burst Lightning",
        CardType::Instant,
        Color::Red,
        mana_cost(0, &[(Color::Red, 1)]),
        effect,
    )
    .with_text("Kicker {4} (You may pay an additional {4} as you cast this spell.)\nBurst Lightning deals 2 damage to any target. If this spell was kicked, it deals 4 damage instead.");
    def.abilities.push(Ability::Kicker { cost: mana_cost(4, &[]) });
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, ConfirmKind, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::PlayerId;
    use crate::priority::Engine;

    /// Targets the opponent (player); answers the kicker Confirm with `pay_kicker`.
    #[derive(Clone)]
    struct BurstAgent {
        pay_kicker: bool,
    }
    impl Agent for BurstAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { kind: ConfirmKind::Generic } => DecisionResponse::Bool(self.pay_kicker),
                DecisionRequest::ChooseTargets { slots, .. } if !slots.is_empty() => {
                    // Prefer targeting the opponent player.
                    let pick = slots[0].legal.iter().position(|t| *t == Target::Player(PlayerId(1))).unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, pick as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn cast_with(mountains: usize, pay_kicker: bool) -> Engine {
        let mut state = build_game(1, &[&[], &[]]);
        let bolt = state.add_card(PlayerId(0), state.card_db().get(BURST_LIGHTNING).unwrap().chars.clone(), Zone::Hand);
        for _ in 0..mountains {
            state.add_card(PlayerId(0), state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone(), Zone::Battlefield);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(BurstAgent { pay_kicker }), Box::new(BurstAgent { pay_kicker })]);
        e.cast_spell(PlayerId(0), bolt, CastVariant::Normal);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        e
    }

    #[test]
    fn unkicked_deals_two() {
        // Only {R} available → can't kick; deals 2.
        let e = cast_with(1, true);
        assert_eq!(e.state.player(PlayerId(1)).life, 18, "2 damage (not kicked — couldn't afford {{4}})");
    }

    #[test]
    fn kicked_deals_four() {
        // {R} + {4} available and the caster pays the kicker → deals 4.
        let e = cast_with(5, true);
        assert_eq!(e.state.player(PlayerId(1)).life, 16, "kicked → 4 damage");
    }

    #[test]
    fn affordable_but_declined_deals_two() {
        // Enough mana to kick, but the caster declines → deals 2.
        let e = cast_with(5, false);
        assert_eq!(e.state.player(PlayerId(1)).life, 18, "declined kicker → 2 damage");
    }
}
