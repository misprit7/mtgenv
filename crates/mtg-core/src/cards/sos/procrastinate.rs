//! Procrastinate — `{X}{U}` Sorcery (first printed SOS).
//!
//! Oracle: "Tap target creature. Put twice X stun counters on it. (If a permanent with a stun counter
//! would become untapped, remove one from it instead.)"
//!
//! **Fully implemented** — taps a target creature and loads it with `2·X` stun counters. The stun
//! rule (CR 702.171) is enforced at the untap step (priority.rs): a permanent that would untap has a
//! stun counter removed and stays tapped instead.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const PROCRASTINATE: u32 = 282;

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
        Effect::PutCounters {
            what: EffectTarget::ChosenIndex(0),
            kind: CounterKind::Stun,
            n: ValueExpr::XTimes(2),
        },
    ]);
    let mut cost = mana_cost(0, &[(Color::Blue, 1)]);
    cost.x = 1;
    db.insert(
        spell(PROCRASTINATE, "Procrastinate", CardType::Sorcery, Color::Blue, cost, effect)
            .with_text("Tap target creature. Put twice X stun counters on it. (If a permanent with a stun counter would become untapped, remove one from it instead.)"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn procrastinate_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PROCRASTINATE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert!(def.fully_implemented);
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
                    PutCounters {
                        what: ChosenIndex(
                            0,
                        ),
                        kind: Stun,
                        n: XTimes(
                            2,
                        ),
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Resolve with X=2: the target is tapped and gets 4 stun counters.
    #[test]
    fn procrastinate_taps_and_stuns() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bear = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(PROCRASTINATE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), x: Some(2), chosen_targets: vec![Target::Object(bear)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.objects.get(&bear).unwrap().status.tapped, "target tapped");
        assert_eq!(e.state.objects.get(&bear).unwrap().counters.get(&CounterKind::Stun), 4, "2·X = 4 stun counters");
    }

    /// S3 cap: at the untap step, a permanent with stun counters stays tapped and loses one stun
    /// counter each turn instead of untapping; it untaps normally once the counters are gone.
    #[test]
    fn stun_counter_replaces_untap() {
        use crate::agent::RandomAgent;
        use crate::basics::{Phase, Zone};
        use crate::cards::{build_game, grp};
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        state.active_player = PlayerId(0);
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        // Tapped, with 2 stun counters.
        if let Some(o) = state.objects.get_mut(&bear) {
            o.status.tapped = true;
            o.counters.counts.insert(CounterKind::Stun, 2);
        }
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let stun = |e: &Engine| e.state.objects.get(&bear).unwrap().counters.get(&CounterKind::Stun);
        let tapped = |e: &Engine| e.state.objects.get(&bear).unwrap().status.tapped;

        e.run_step(Phase::Untap);
        assert!(tapped(&e) && stun(&e) == 1, "1st untap: stays tapped, one stun removed");
        e.run_step(Phase::Untap);
        assert!(tapped(&e) && stun(&e) == 0, "2nd untap: stays tapped, last stun removed");
        e.run_step(Phase::Untap);
        assert!(!tapped(&e), "3rd untap: no stun counters left → untaps normally");
    }
}
