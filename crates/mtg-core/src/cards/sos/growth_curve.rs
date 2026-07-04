//! Growth Curve — `{G}{U}` Sorcery (first printed SOS).
//!
//! Oracle: "Put a +1/+1 counter on target creature you control, then double the number of +1/+1
//! counters on that creature."
//!
//! **Fully implemented** — one declared target ("target creature you control", slot 0) gets a +1/+1
//! counter (`PutCounters{ Fixed(1) }`), then a second `PutCounters` on the SAME target
//! (`ChosenIndex(0)`) adds `CountersOnTarget{0, +1/+1}` more — doubling the count. The "then double"
//! reads the count AFTER the first counter commits: the new `PutCounters` interpret arm flushes
//! staged actions before it runs (#61 deferred→imperative ordering), so the doubling step sees the
//! post-first-counter total. Start with C counters → C+1 → 2(C+1).

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const GROWTH_CURVE: u32 = 351;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::PutCounters {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
                min: 1,
                max: 1,
                distinct: true,
            }),
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::Fixed(1),
        },
        // "then double" — put as many more +1/+1 counters as are now on it (reads the post-first
        // count, so it doubles). Same target as slot 0 via `ChosenIndex(0)` (no new target).
        Effect::PutCounters {
            what: EffectTarget::ChosenIndex(0),
            kind: CounterKind::PlusOnePlusOne,
            n: ValueExpr::CountersOnTarget { target: 0, kind: CounterKind::PlusOnePlusOne },
        },
    ]);
    let mut def = spell(
        GROWTH_CURVE,
        "Growth Curve",
        CardType::Sorcery,
        Color::Green,
        mana_cost(0, &[(Color::Green, 1), (Color::Blue, 1)]),
        effect,
    )
    .with_text(
        "Put a +1/+1 counter on target creature you control, then double the number of +1/+1 counters on that creature.",
    );
    def.chars.colors = vec![Color::Green, Color::Blue];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use expect_test::expect;

    #[test]
    fn growth_curve_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(GROWTH_CURVE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.colors, vec![Color::Green, Color::Blue]);
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    PutCounters {
                        what: Target(
                            TargetSpec {
                                kind: Creature(
                                    ControlledBy(
                                        Controller,
                                    ),
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        kind: PlusOnePlusOne,
                        n: Fixed(
                            1,
                        ),
                    },
                    PutCounters {
                        what: ChosenIndex(
                            0,
                        ),
                        kind: PlusOnePlusOne,
                        n: CountersOnTarget {
                            target: 0,
                            kind: PlusOnePlusOne,
                        },
                    },
                ],
            )"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: a creature with 2 existing +1/+1 counters. Growth Curve puts one (→ 3), then doubles
    /// (→ 6). Proves the doubling reads the post-first-counter count via the flush.
    #[test]
    fn puts_one_then_doubles() {
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let target = state.add_card(PlayerId(0), bears, Zone::Battlefield);
        // Seed two existing +1/+1 counters.
        state.objects.get_mut(&target).unwrap().counters.counts.insert(CounterKind::PlusOnePlusOne, 2);
        let effect = state.card_db().get(GROWTH_CURVE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        assert_eq!(e.state.object(target).counters.get(&CounterKind::PlusOnePlusOne), 2);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(target)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(
            e.state.object(target).counters.get(&CounterKind::PlusOnePlusOne),
            6,
            "2 → +1 → 3 → doubled → 6"
        );
    }

    /// On a creature with no counters: +1 → 1, doubled → 2 (a fresh 2/2 becomes a 4/4).
    #[test]
    fn from_zero_counters() {
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let target = state.add_card(PlayerId(0), bears, Zone::Battlefield);
        let effect = state.card_db().get(GROWTH_CURVE).unwrap().spell_effect().unwrap().clone();
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
        assert_eq!(
            e.state.object(target).counters.get(&CounterKind::PlusOnePlusOne),
            2,
            "0 → +1 → 1 → doubled → 2"
        );
        let cc = e.state.computed(target);
        assert_eq!((cc.power, cc.toughness), (Some(4), Some(4)), "2/2 base + 2 counters = 4/4");
    }
}
