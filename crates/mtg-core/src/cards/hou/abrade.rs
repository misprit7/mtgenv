//! Abrade — `{1}{R}` Instant (first printed HOU; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Choose one —
//! • Abrade deals 3 damage to target creature.
//! • Destroy target artifact."
//!
//! **Fully implemented** — a `Modal` "choose one": 3 damage to a target creature, or destroy a target
//! artifact.

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget, Mode};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const ABRADE: u32 = 619;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "Abrade deals 3 damage to target creature".to_string(),
                effect: Effect::DealDamage {
                    amount: ValueExpr::Fixed(3),
                    to: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Creature(CardFilter::Any),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                    kind: DamageKind::Noncombat,
                },
            },
            Mode {
                label: "Destroy target artifact".to_string(),
                effect: Effect::Destroy {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Permanent(CardFilter::HasCardType(CardType::Artifact)),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                },
            },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    db.insert(
        spell(
            ABRADE,
            "Abrade",
            CardType::Instant,
            Color::Red,
            mana_cost(1, &[(Color::Red, 1)]),
            effect,
        )
        .with_text("Choose one —\n• Abrade deals 3 damage to target creature.\n• Destroy target artifact."),
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

    #[test]
    fn abrade_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ABRADE).unwrap();
        assert!(def.fully_implemented);
        match def.spell_effect().unwrap() {
            Effect::Modal { modes, min, max, .. } => assert_eq!((modes.len(), *min, *max), (2, 1, 1)),
            o => panic!("expected Modal, got {o:?}"),
        }
    }

    /// Mode 1: 3 damage marked on a target creature.
    #[test]
    fn mode1_deals_3_to_a_creature() {
        let mut state = build_game(1, &[&[], &[]]);
        let giant = state.add_card(PlayerId(1), state.card_db().get(grp::HILL_GIANT).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(ABRADE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_modes: vec![0], chosen_targets: vec![Target::Object(giant)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(giant).damage_marked, 3, "3 damage marked");
    }
}
