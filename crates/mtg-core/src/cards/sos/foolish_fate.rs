//! Foolish Fate — `{2}{B}` Instant (first printed SOS).
//!
//! Oracle: "Destroy target creature. Infusion — If you gained life this turn, that creature's
//! controller loses 3 life."
//!
//! **Fully implemented** — `Destroy` the target creature, then a `Conditional` on the Infusion gate
//! (`GainedLifeThisTurn`): if you gained life this turn, the destroyed creature's controller
//! (`ControllerOfTarget(0)`, snapshotted before the destroy) loses 3 life.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const FOOLISH_FATE: u32 = 236;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Destroy {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
        Effect::Conditional {
            cond: Condition::GainedLifeThisTurn { who: PlayerRef::Controller },
            then: Box::new(Effect::LoseLife {
                who: PlayerRef::ControllerOfTarget(0),
                amount: ValueExpr::Fixed(3),
            }),
            otherwise: None,
        },
    ]);
    db.insert(
        spell(
            FOOLISH_FATE,
            "Foolish Fate",
            CardType::Instant,
            Color::Black,
            mana_cost(2, &[(Color::Black, 1)]),
            effect,
        )
        .with_text("Destroy target creature. Infusion — If you gained life this turn, that creature's controller loses 3 life."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn foolish_fate_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(FOOLISH_FATE).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    Destroy {
                        what: Target(
                            TargetSpec {
                                kind: Creature(
                                    Any,
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                    },
                    Conditional {
                        cond: GainedLifeThisTurn {
                            who: Controller,
                        },
                        then: LoseLife {
                            who: ControllerOfTarget(
                                0,
                            ),
                            amount: Fixed(
                                3,
                            ),
                        },
                        otherwise: None,
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: with Infusion active (gained life this turn) the destroyed creature's controller
    /// loses 3; without it, only the destroy happens.
    #[test]
    fn foolish_fate_infusion_drains_when_life_gained() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        let cast = |life_gained: u32| {
            let mut state = build_game(1, &[&[], &[]]);
            state.players[0].life_gained_this_turn = life_gained;
            let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            let victim = state.add_card(PlayerId(1), bears, Zone::Battlefield);
            let effect = state.card_db().get(FOOLISH_FATE).unwrap().spell_effect().unwrap().clone();
            let mut e = Engine::new(
                state,
                vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
            );
            let p1 = e.state.player(PlayerId(1)).life;
            e.resolve_effect(
                &effect,
                &ResolutionCtx {
                    controller: Some(PlayerId(0)),
                    chosen_targets: vec![Target::Object(victim)],
                    target_controllers: vec![Some(PlayerId(1))],
                    ..Default::default()
                },
                WbReason::Resolve(StackId(0)),
            );
            assert!(e.state.players[1].graveyard.contains(&victim), "creature destroyed either way");
            p1 - e.state.player(PlayerId(1)).life
        };
        assert_eq!(cast(2), 3, "gained life → controller loses 3");
        assert_eq!(cast(0), 0, "no life gained → no drain");
    }
}
