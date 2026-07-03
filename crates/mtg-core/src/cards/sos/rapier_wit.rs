//! Rapier Wit — `{1}{W}` Instant (first printed SOS).
//!
//! Oracle: "Tap target creature. If it's your turn, put a stun counter on it. Draw a card."
//!
//! **Fully implemented** — tap a target creature, add a stun counter only on your turn (`YourTurn`
//! `Conditional`), then draw.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::condition::Condition;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const RAPIER_WIT: u32 = 285;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Tap {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            tap: true,
        },
        Effect::Conditional {
            cond: Condition::YourTurn,
            then: Box::new(Effect::PutCounters {
                what: EffectTarget::ChosenIndex(0),
                kind: CounterKind::Stun,
                n: ValueExpr::Fixed(1),
            }),
            otherwise: None,
        },
        Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
    ]);
    db.insert(
        spell(RAPIER_WIT, "Rapier Wit", CardType::Instant, Color::White, mana_cost(1, &[(Color::White, 1)]), effect)
            .with_text("Tap target creature. If it's your turn, put a stun counter on it.\nDraw a card."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn rapier_wit_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        assert!(db.get(RAPIER_WIT).unwrap().fully_implemented);
        expect![[r#"
            Sequence(
                [
                    Tap {
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
                        tap: true,
                    },
                    Conditional {
                        cond: YourTurn,
                        then: PutCounters {
                            what: ChosenIndex(
                                0,
                            ),
                            kind: Stun,
                            n: Fixed(
                                1,
                            ),
                        },
                        otherwise: None,
                    },
                    Draw {
                        who: Controller,
                        count: Fixed(
                            1,
                        ),
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", db.get(RAPIER_WIT).unwrap().spell_effect().unwrap()));
    }

    #[test]
    fn rapier_wit_taps_stuns_on_your_turn_and_draws() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let lib = vec![grp::FOREST];
        let mut state = build_game(1, &[&lib, &[]]);
        state.active_player = PlayerId(0); // your turn
        let bear = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(RAPIER_WIT).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let hand0 = e.state.players[0].hand.len();
        e.resolve_effect(&effect, &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(bear)], ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert!(e.state.objects.get(&bear).unwrap().status.tapped, "tapped");
        assert_eq!(e.state.objects.get(&bear).unwrap().counters.get(&CounterKind::Stun), 1, "stun on your turn");
        assert_eq!(e.state.players[0].hand.len(), hand0 + 1, "drew a card");
    }
}
