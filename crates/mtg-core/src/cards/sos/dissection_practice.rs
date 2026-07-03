//! Dissection Practice — `{B}` Instant (first printed SOS).
//!
//! Oracle: "Target opponent loses 1 life and you gain 1 life. / Up to one target creature gets +1/+1
//! until end of turn. / Up to one target creature gets -1/-1 until end of turn."
//!
//! **Fully implemented** — a drain (the single opponent loses 1, you gain 1) plus two independent
//! "up to one target creature" pumps (+1/+1 and -1/-1 until end of turn). ("Target opponent" is the
//! forced single opponent in 2-player, so it's `PlayerRef::Opponent` rather than a target slot.)

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const DISSECTION_PRACTICE: u32 = 271;

fn up_to_one_creature() -> EffectTarget {
    EffectTarget::Target(TargetSpec {
        kind: TargetKind::Creature(CardFilter::Any),
        min: 0,
        max: 1,
        distinct: true,
    })
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::LoseLife { who: PlayerRef::Opponent, amount: ValueExpr::Fixed(1) },
        Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
        Effect::PumpPT {
            what: up_to_one_creature(),
            power: ValueExpr::Fixed(1),
            toughness: ValueExpr::Fixed(1),
            duration: Duration::UntilEndOfTurn,
        },
        Effect::PumpPT {
            what: up_to_one_creature(),
            power: ValueExpr::Fixed(-1),
            toughness: ValueExpr::Fixed(-1),
            duration: Duration::UntilEndOfTurn,
        },
    ]);
    db.insert(
        spell(
            DISSECTION_PRACTICE,
            "Dissection Practice",
            CardType::Instant,
            Color::Black,
            mana_cost(0, &[(Color::Black, 1)]),
            effect,
        )
        .with_text("Target opponent loses 1 life and you gain 1 life.\nUp to one target creature gets +1/+1 until end of turn.\nUp to one target creature gets -1/-1 until end of turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn dissection_practice_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        assert!(db.get(DISSECTION_PRACTICE).unwrap().fully_implemented);
        expect![[r#"
            Sequence(
                [
                    LoseLife {
                        who: Opponent,
                        amount: Fixed(
                            1,
                        ),
                    },
                    GainLife {
                        who: Controller,
                        amount: Fixed(
                            1,
                        ),
                    },
                    PumpPT {
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
                        power: Fixed(
                            1,
                        ),
                        toughness: Fixed(
                            1,
                        ),
                        duration: UntilEndOfTurn,
                    },
                    PumpPT {
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
                        power: Fixed(
                            -1,
                        ),
                        toughness: Fixed(
                            -1,
                        ),
                        duration: UntilEndOfTurn,
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", db.get(DISSECTION_PRACTICE).unwrap().spell_effect().unwrap()));
    }

    /// Behaviour: drain 1, and the two creature slots buff/shrink independently (a 2/2 pumped +1/+1 →
    /// 3/3, another 2/2 shrunk -1/-1 → 1/1).
    #[test]
    fn dissection_practice_drains_and_pumps() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let up = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let down = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(DISSECTION_PRACTICE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let (p0, p1) = (e.state.player(PlayerId(0)).life, e.state.player(PlayerId(1)).life);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(up), Target::Object(down)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(1)).life, p1 - 1, "opponent loses 1");
        assert_eq!(e.state.player(PlayerId(0)).life, p0 + 1, "you gain 1");
        assert_eq!(e.state.computed(up).power, Some(3), "+1/+1 target → 3/3");
        assert_eq!(e.state.computed(down).power, Some(1), "-1/-1 target → 1/1");
    }
}
