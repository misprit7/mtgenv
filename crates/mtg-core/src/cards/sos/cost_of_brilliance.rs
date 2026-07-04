//! Cost of Brilliance — `{2}{B}` Sorcery (first printed SOS).
//!
//! Oracle: "Target player draws two cards and loses 2 life. Put a +1/+1 counter on up to one target
//! creature."
//!
//! **Fully implemented** — a `TargetPlayer` declaration (slot 0) that the `Draw`/`LoseLife` read via
//! `ChosenTarget(0)`, then a `PutCounters` on "up to one" (slot 1, `min: 0`) target creature.
//! Exercises the player-as-target cap.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, PlayerFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const COST_OF_BRILLIANCE: u32 = 270;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::TargetPlayer(PlayerFilter::Any),
        Effect::Draw { who: PlayerRef::ChosenTarget(0), count: ValueExpr::Fixed(2) },
        Effect::LoseLife { who: PlayerRef::ChosenTarget(0), amount: ValueExpr::Fixed(2) },
        Effect::PutCounters {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 0,
                max: 1,
                distinct: true,
            }),
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::Fixed(1),
        },
    ]);
    db.insert(
        spell(
            COST_OF_BRILLIANCE,
            "Cost of Brilliance",
            CardType::Sorcery,
            Color::Black,
            mana_cost(2, &[(Color::Black, 1)]),
            effect,
        )
        .with_text("Target player draws two cards and loses 2 life. Put a +1/+1 counter on up to one target creature."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn cost_of_brilliance_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        assert!(db.get(COST_OF_BRILLIANCE).unwrap().fully_implemented);
        expect![[r#"
            Sequence(
                [
                    TargetPlayer(
                        Any,
                    ),
                    Draw {
                        who: ChosenTarget(
                            0,
                        ),
                        count: Fixed(
                            2,
                        ),
                    },
                    LoseLife {
                        who: ChosenTarget(
                            0,
                        ),
                        amount: Fixed(
                            2,
                        ),
                    },
                    PutCounters {
                        what: Target(
                            TargetSpec {
                                kind: Creature(
                                    Any,
                                ),
                                min: 0,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        kind: PlusOnePlusOne,
                        n: Fixed(
                            1,
                        ),
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", db.get(COST_OF_BRILLIANCE).unwrap().spell_effect().unwrap()));
    }

    /// Behaviour: the targeted player (here the opponent) draws two and loses 2; the targeted creature
    /// gets a +1/+1 counter. Validates that `ChosenTarget(0)` resolves to the chosen player and the
    /// creature slot lines up after the player-target declaration.
    #[test]
    fn cost_of_brilliance_targets_a_player_and_a_creature() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        // Opponent (P1) has a 3-card library; a creature on P0's side to counter-buff.
        let mut state = build_game(1, &[&[], &[grp::FOREST, grp::FOREST, grp::FOREST]]);
        let bears = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(COST_OF_BRILLIANCE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let (p1_life, p1_hand) = (e.state.player(PlayerId(1)).life, e.state.players[1].hand.len());
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                // slot 0 = the targeted player (the opponent), slot 1 = the creature.
                chosen_targets: vec![Target::Player(PlayerId(1)), Target::Object(bears)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.players[1].hand.len(), p1_hand + 2, "targeted player drew two");
        assert_eq!(e.state.player(PlayerId(1)).life, p1_life - 2, "targeted player lost 2 life");
        assert_eq!(e.state.computed(bears).power, Some(3), "the target creature got a +1/+1 counter");
    }
}
