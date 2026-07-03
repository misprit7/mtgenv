//! Efflorescence — `{2}{G}` Instant (first printed SOS).
//!
//! Oracle: "Put two +1/+1 counters on target creature. Infusion — If you gained life this turn,
//! that creature also gains trample and indestructible until end of turn."
//!
//! **Fully implemented** — `PutCounters` (+1/+1 ×2) on the target creature (slot 0), then a
//! `Conditional` on the Infusion gate that grants that same creature (`ChosenIndex(0)`) trample and
//! indestructible until end of turn. The conditional's body declares no new target, so it resolves
//! inline (not as a reflexive sub-trigger).

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const EFFLORESCENCE: u32 = 239;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::PutCounters {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::Fixed(2),
        },
        Effect::Conditional {
            cond: Condition::GainedLifeThisTurn { who: PlayerRef::Controller },
            then: Box::new(Effect::Sequence(vec![
                Effect::GrantKeyword {
                    what: EffectTarget::ChosenIndex(0),
                    keyword: Keyword::Trample,
                    duration: Duration::UntilEndOfTurn,
                },
                Effect::GrantKeyword {
                    what: EffectTarget::ChosenIndex(0),
                    keyword: Keyword::Indestructible,
                    duration: Duration::UntilEndOfTurn,
                },
            ])),
            otherwise: None,
        },
    ]);
    db.insert(
        spell(
            EFFLORESCENCE,
            "Efflorescence",
            CardType::Instant,
            Color::Green,
            mana_cost(2, &[(Color::Green, 1)]),
            effect,
        )
        .with_text("Put two +1/+1 counters on target creature. Infusion — If you gained life this turn, that creature also gains trample and indestructible until end of turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn efflorescence_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(EFFLORESCENCE).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    PutCounters {
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
                        kind: PlusOnePlusOne,
                        n: Fixed(
                            2,
                        ),
                    },
                    Conditional {
                        cond: GainedLifeThisTurn {
                            who: Controller,
                        },
                        then: Sequence(
                            [
                                GrantKeyword {
                                    what: ChosenIndex(
                                        0,
                                    ),
                                    keyword: Trample,
                                    duration: UntilEndOfTurn,
                                },
                                GrantKeyword {
                                    what: ChosenIndex(
                                        0,
                                    ),
                                    keyword: Indestructible,
                                    duration: UntilEndOfTurn,
                                },
                            ],
                        ),
                        otherwise: None,
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: always +2/+2; the trample+indestructible grant only lands when life was gained.
    #[test]
    fn efflorescence_counters_always_grants_on_infusion() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        let run = |life_gained: u32| {
            let mut state = build_game(1, &[&[], &[]]);
            state.players[0].life_gained_this_turn = life_gained;
            let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            let target = state.add_card(PlayerId(0), bears, Zone::Battlefield);
            let effect = state.card_db().get(EFFLORESCENCE).unwrap().spell_effect().unwrap().clone();
            let mut e = Engine::new(
                state,
                vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
            );
            e.resolve_effect(
                &effect,
                &ResolutionCtx {
                    controller: Some(PlayerId(0)),
                    chosen_targets: vec![Target::Object(target)],
                    ..Default::default()
                },
                WbReason::Resolve(StackId(0)),
            );
            let cc = e.state.computed(target);
            (cc.power, cc.has_keyword(Keyword::Trample), cc.has_keyword(Keyword::Indestructible))
        };
        assert_eq!(run(2), (Some(4), true, true), "gained life → 4/4 with trample + indestructible");
        assert_eq!(run(0), (Some(4), false, false), "no life gained → just the +2/+2 counters");
    }
}
