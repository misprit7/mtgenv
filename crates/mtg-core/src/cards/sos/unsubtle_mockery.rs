//! Unsubtle Mockery — `{2}{R}` Instant (first printed SOS).
//!
//! Oracle: "Unsubtle Mockery deals 4 damage to target creature. Surveil 1."
//!
//! **Fully implemented** — `DealDamage 4` to a target creature, then `Surveil 1`.

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const UNSUBTLE_MOCKERY: u32 = 233;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::DealDamage {
            amount: ValueExpr::Fixed(4),
            to: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            kind: DamageKind::Noncombat,
        },
        Effect::Surveil { count: ValueExpr::Fixed(1) },
    ]);
    db.insert(
        spell(
            UNSUBTLE_MOCKERY,
            "Unsubtle Mockery",
            CardType::Instant,
            Color::Red,
            mana_cost(2, &[(Color::Red, 1)]),
            effect,
        )
        .with_text("Unsubtle Mockery deals 4 damage to target creature. Surveil 1."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn unsubtle_mockery_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(UNSUBTLE_MOCKERY).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    DealDamage {
                        amount: Fixed(
                            4,
                        ),
                        to: Target(
                            TargetSpec {
                                kind: Creature(
                                    Any,
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        kind: Noncombat,
                    },
                    Surveil {
                        count: Fixed(
                            1,
                        ),
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: 4 damage kills a 2/2, and surveil-1 keeping the top card leaves library/graveyard as-is.
    #[test]
    fn unsubtle_mockery_burns_and_surveils() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        // Keeps everything on top (bins nothing on surveil).
        #[derive(Clone)]
        struct KeepAll;
        impl Agent for KeepAll {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![]),
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = build_game(1, &[&[grp::FOREST], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let victim = state.add_card(PlayerId(1), bears, Zone::Battlefield);
        let effect = state.card_db().get(UNSUBTLE_MOCKERY).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(KeepAll), Box::new(KeepAll)]);
        let lib_before = e.state.players[0].library.len();
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(victim)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        e.run_agenda(); // lethal-damage SBA
        assert!(e.state.players[1].graveyard.contains(&victim), "4 damage killed the 2/2");
        // Surveil kept the card on top: library unchanged, nothing binned to our graveyard.
        assert_eq!(e.state.players[0].library.len(), lib_before, "surveil kept the top card");
        assert!(e.state.players[0].graveyard.is_empty(), "nothing binned to our graveyard");
    }
}
