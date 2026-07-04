//! Homesickness — `{4}{U}{U}` Instant (first printed SOS).
//!
//! Oracle: "Target player draws two cards. Tap up to two target creatures. Put a stun counter on each
//! of them. (If a permanent with a stun counter would become untapped, remove one from it instead.)"
//!
//! **Fully implemented** — exercises the new `Effect::ForEachTarget` cap (apply a body to each chosen
//! target of a *variable* multi-target slot). Structure:
//! - `TargetPlayer` (slot 0) → `Draw { ChosenTarget(0), 2 }` (the player-as-target cap, cf. Cost of
//!   Brilliance).
//! - `ForEachTarget { slot: up-to-2 creatures, body }` (slot 1): each chosen creature is bound to
//!   `EffectTarget::Each` in turn while `body` = `Tap{Each}` + `PutCounters{Each, Stun}` runs — so
//!   "tap up to two target creatures. Put a stun counter on each of them." A `min: 0` slot means the
//!   caster may pick 0, 1, or 2 creatures; the loop applies to exactly those chosen. Stun counters
//!   (S3) already have the untap-step replacement.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, PlayerFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const HOMESICKNESS: u32 = 343;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::TargetPlayer(PlayerFilter::Any),
        Effect::Draw { who: PlayerRef::ChosenTarget(0), count: ValueExpr::Fixed(2) },
        Effect::ForEachTarget {
            slot: TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 0,
                max: 2,
                distinct: true,
            },
            body: Box::new(Effect::Sequence(vec![
                Effect::Tap { what: EffectTarget::Each, tap: true },
                Effect::PutCounters {
                    what: EffectTarget::Each,
                    kind: CounterKind::Stun,
                    n: ValueExpr::Fixed(1),
                },
            ])),
        },
    ]);
    db.insert(
        spell(
            HOMESICKNESS,
            "Homesickness",
            CardType::Instant,
            Color::Blue,
            mana_cost(4, &[(Color::Blue, 2)]),
            effect,
        )
        .with_text(
            "Target player draws two cards. Tap up to two target creatures. Put a stun counter on each of them. (If a permanent with a stun counter would become untapped, remove one from it instead.)",
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::{target_specs_for, Engine};
    use expect_test::expect;

    #[test]
    fn homesickness_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(HOMESICKNESS).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert_eq!(def.chars.colors, vec![Color::Blue]);
        assert!(def.fully_implemented);
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
                    ForEachTarget {
                        slot: TargetSpec {
                            kind: Creature(
                                Any,
                            ),
                            min: 0,
                            max: 2,
                            distinct: true,
                        },
                        body: Sequence(
                            [
                                Tap {
                                    what: Each,
                                    tap: true,
                                },
                                PutCounters {
                                    what: Each,
                                    kind: Stun,
                                    n: Fixed(
                                        1,
                                    ),
                                },
                            ],
                        ),
                    },
                ],
            )"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// The engine collects the right targeting slots at cast (CR 601.2c): a single Player, then an
    /// "up to two" creature slot. Proves `ForEachTarget`'s slot is enumerated by `collect_specs_into`.
    #[test]
    fn declares_a_player_then_up_to_two_creatures() {
        let mut db = CardDb::default();
        register(&mut db);
        let effect = db.get(HOMESICKNESS).unwrap().spell_effect().unwrap().clone();
        let specs = target_specs_for(&effect, &[]);
        assert_eq!(specs.len(), 2, "one player slot + one creature slot");
        assert!(matches!(specs[0].kind, TargetKind::Player(_)), "slot 0 = target player");
        assert!(matches!(specs[1].kind, TargetKind::Creature(_)), "slot 1 = target creature(s)");
        assert_eq!((specs[1].min, specs[1].max), (0, 2), "up to two creatures");
    }

    /// Resolve with the full set: the target player draws two, and BOTH chosen creatures are tapped
    /// and get a stun counter (the per-target body applied to each of the variable slot).
    #[test]
    fn taps_and_stuns_both_targets_and_draws() {
        let mut state = build_game(1, &[&[], &[]]);
        // Two library cards for P0 to draw.
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        state.add_card(PlayerId(0), bears.clone(), Zone::Library);
        state.add_card(PlayerId(0), bears.clone(), Zone::Library);
        let a = state.add_card(PlayerId(1), bears.clone(), Zone::Battlefield);
        let b = state.add_card(PlayerId(1), bears, Zone::Battlefield);
        let effect = state.card_db().get(HOMESICKNESS).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let hand_before = e.state.player(PlayerId(0)).hand.len();
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Player(PlayerId(0)), Target::Object(a), Target::Object(b)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand_before + 2, "target player drew two");
        for id in [a, b] {
            assert!(e.state.object(id).status.tapped, "creature tapped");
            assert_eq!(e.state.object(id).counters.get(&CounterKind::Stun), 1, "creature stunned");
        }
    }

    /// The "up to two" slot is variable: choosing ONE creature taps/stuns only that one (the loop
    /// stops when the chosen targets run out).
    #[test]
    fn one_target_creature_only() {
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        state.add_card(PlayerId(0), bears.clone(), Zone::Library);
        state.add_card(PlayerId(0), bears.clone(), Zone::Library);
        let a = state.add_card(PlayerId(1), bears.clone(), Zone::Battlefield);
        let untouched = state.add_card(PlayerId(1), bears, Zone::Battlefield);
        let effect = state.card_db().get(HOMESICKNESS).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Player(PlayerId(0)), Target::Object(a)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.object(a).status.tapped, "the one chosen creature is tapped");
        assert_eq!(e.state.object(a).counters.get(&CounterKind::Stun), 1, "and stunned");
        assert!(!e.state.object(untouched).status.tapped, "the unchosen creature is untouched");
        assert_eq!(e.state.object(untouched).counters.get(&CounterKind::Stun), 0, "no stun on it");
    }
}
