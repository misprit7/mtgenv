//! Burrog Barrage — `{1}{G}` Instant (first printed SOS).
//!
//! Oracle: "Target creature you control gets +1/+0 until end of turn if you've cast another instant
//! or sorcery spell this turn. Then it deals damage equal to its power to up to one target creature
//! an opponent controls."
//!
//! **Fully implemented.** Two declared targets — a mandatory "creature you control" (slot 0) and an
//! "up to one" opponent creature (slot 1) — so the effect is a `Sequence`:
//!  1. A `Conditional` gated on `ValueAtLeast(InstantsSorceriesCastThisTurn{Controller}, 2)` — the
//!     counter increments at cast, so the resolving Burrog counts itself; "**another**" I/S ⟺ ≥2 —
//!     whose `then` is a `PumpPT{ what: ChosenIndex(0), +1/+0, until end of turn }`. Referencing the
//!     shared slot 0 by index (not a fresh `Target`) keeps it out of the cast-time target collection
//!     (which the `Conditional` wouldn't walk anyway) — slot 0 is declared by step 2.
//!  2. A `SourcedDamage{ source: creature you control (slot 0), to: up-to-one opponent creature
//!     (slot 1), amount: PowerOfTarget(0) }` — "**it** deals damage equal to its power": the buffed
//!     creature is the damage source (CR 119.2). Its flushing interpret arm commits step 1's pump
//!     first, so `PowerOfTarget(0)` reads the boosted power, and the whole thing is faithful whether
//!     or not the opponent-creature slot is chosen (declined ⇒ no damage; the +1/+0 still applies).

use crate::basics::{CardType, Color, DamageKind};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const BURROG_BARRAGE: u32 = 444;

pub fn register(db: &mut CardDb) {
    // Slot 0: the creature you control (mandatory). Declared by the `SourcedDamage.source` below; the
    // pump references it by index so the conditional bonus doesn't spawn a second target.
    let you_control = TargetSpec {
        kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
        min: 1,
        max: 1,
        distinct: true,
    };
    // Slot 1: up to one creature an opponent controls (the damage recipient).
    let opp_creature = TargetSpec {
        kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Opponent)),
        min: 0,
        max: 1,
        distinct: true,
    };
    let effect = Effect::Sequence(vec![
        // "gets +1/+0 until end of turn if you've cast another instant or sorcery spell this turn"
        Effect::Conditional {
            cond: Condition::ValueAtLeast(
                ValueExpr::InstantsSorceriesCastThisTurn { who: PlayerRef::Controller },
                ValueExpr::Fixed(2),
            ),
            then: Box::new(Effect::PumpPT {
                what: EffectTarget::ChosenIndex(0),
                power: ValueExpr::Fixed(1),
                toughness: ValueExpr::Fixed(0),
                duration: Duration::UntilEndOfTurn,
            }),
            otherwise: None,
        },
        // "Then it deals damage equal to its power to up to one target creature an opponent controls."
        Effect::SourcedDamage {
            source: EffectTarget::Target(you_control),
            to: EffectTarget::Target(opp_creature),
            amount: ValueExpr::PowerOfTarget(0),
            kind: DamageKind::Noncombat,
        },
    ]);
    let mut def = spell(
        BURROG_BARRAGE,
        "Burrog Barrage",
        CardType::Instant,
        Color::Green,
        mana_cost(1, &[(Color::Green, 1)]),
        effect,
    )
    .with_text("Target creature you control gets +1/+0 until end of turn if you've cast another instant or sorcery spell this turn. Then it deals damage equal to its power to up to one target creature an opponent controls.");
    def.fully_implemented = true;
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;

    #[test]
    fn burrog_barrage_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BURROG_BARRAGE).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        // Sequence: conditional pump (slot 0 by index) then sourced damage (slots 0 → 1).
        expect_test::expect![[r#"
            Sequence(
                [
                    Conditional {
                        cond: ValueAtLeast(
                            InstantsSorceriesCastThisTurn {
                                who: Controller,
                            },
                            Fixed(
                                2,
                            ),
                        ),
                        then: PumpPT {
                            what: ChosenIndex(
                                0,
                            ),
                            power: Fixed(
                                1,
                            ),
                            toughness: Fixed(
                                0,
                            ),
                            duration: UntilEndOfTurn,
                        },
                        otherwise: None,
                    },
                    SourcedDamage {
                        source: Target(
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
                        to: Target(
                            TargetSpec {
                                kind: Creature(
                                    ControlledBy(
                                        Opponent,
                                    ),
                                ),
                                min: 0,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        amount: PowerOfTarget(
                            0,
                        ),
                        kind: Noncombat,
                    },
                ],
            )"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Picks the desired object per slot; returns pairs in slot order (0 then 1). Slot 1 (the
    /// opponent creature) is optionally targeted via `want_opp`.
    #[derive(Clone)]
    struct TargetAgent {
        want_mine: ObjId,
        want_opp: Option<ObjId>,
    }
    impl Agent for TargetAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let mut pairs = Vec::new();
                    // slot 0 — creature you control (mandatory).
                    let i = slots[0]
                        .legal
                        .iter()
                        .position(|t| matches!(t, Target::Object(o) if *o == self.want_mine))
                        .unwrap_or(0);
                    pairs.push((0u32, i as u32));
                    // slot 1 — up-to-one opponent creature (only if we want one and it's legal).
                    if let Some(opp) = self.want_opp {
                        if let Some(j) = slots.get(1).and_then(|s| {
                            s.legal.iter().position(|t| matches!(t, Target::Object(o) if *o == opp))
                        }) {
                            pairs.push((1u32, j as u32));
                        }
                    }
                    DecisionResponse::Pairs(pairs)
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn add(state: &mut crate::state::GameState, who: PlayerId, grp_id: u32, zone: Zone) -> ObjId {
        let c = state.card_db().get(grp_id).unwrap().chars.clone();
        state.add_card(who, c, zone)
    }

    /// The core case: with **another** instant/sorcery already cast this turn, Burrog's target 2/2
    /// gets +1/+0 (→ 3/2) and then deals 3 (its buffed power) to the opponent's 2/2, killing it.
    #[test]
    fn pumps_when_another_is_cast_and_bites_with_buffed_power() {
        let mut state = build_game(1, &[&[], &[]]);
        let mine = add(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        let theirs = add(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        let burrog = add(&mut state, PlayerId(0), BURROG_BARRAGE, Zone::Hand);
        for grp_id in [grp::FOREST, grp::FOREST] {
            add(&mut state, PlayerId(0), grp_id, Zone::Battlefield); // {1}{G}
        }
        // "another instant or sorcery this turn" — pretend one was already cast (counter would be 2
        // after Burrog itself increments it at cast).
        state.player_mut(PlayerId(0)).instants_sorceries_cast_this_turn = 1;
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let agent = TargetAgent { want_mine: mine, want_opp: Some(theirs) };
        let mut e = Engine::new(state, vec![Box::new(agent.clone()), Box::new(agent)]);

        e.cast_spell(PlayerId(0), burrog, CastVariant::Normal);
        e.resolve_top();
        e.run_agenda(); // collect the lethal-damage SBA

        assert_eq!(e.state.computed(mine).power, Some(3), "your creature got +1/+0 (2 → 3)");
        assert_eq!(e.state.object(theirs).zone, Zone::Graveyard, "it took 3 and died");
    }

    /// No **other** I/S cast (only Burrog itself) → no +1/+0; the 2/2 deals its base 2, which is NOT
    /// lethal to a 2/2 that already… actually 2 IS lethal to a 2/2, so use a 3/3 opponent to show the
    /// missing pump: base 2 damage leaves a 3/3 alive.
    #[test]
    fn no_pump_without_another_instant_or_sorcery() {
        let mut state = build_game(1, &[&[], &[]]);
        let mine = add(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        let theirs = add(&mut state, PlayerId(1), grp::HILL_GIANT, Zone::Battlefield); // 3/3
        let burrog = add(&mut state, PlayerId(0), BURROG_BARRAGE, Zone::Hand);
        for grp_id in [grp::FOREST, grp::FOREST] {
            add(&mut state, PlayerId(0), grp_id, Zone::Battlefield);
        }
        // No other I/S cast this turn — casting Burrog makes the counter exactly 1 (< 2).
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let agent = TargetAgent { want_mine: mine, want_opp: Some(theirs) };
        let mut e = Engine::new(state, vec![Box::new(agent.clone()), Box::new(agent)]);

        e.cast_spell(PlayerId(0), burrog, CastVariant::Normal);
        e.resolve_top();
        e.run_agenda();

        assert_eq!(e.state.computed(mine).power, Some(2), "no other I/S cast → no +1/+0");
        assert_eq!(e.state.object(theirs).zone, Zone::Battlefield, "the 3/3 survived 2 base damage");
        assert_eq!(e.state.object(theirs).damage_marked, 2, "took its 2 base power in damage");
    }

    /// "up to one" — the opponent-creature target is optional. Declining it applies the +1/+0 and
    /// deals no damage (a legal, faithful line — e.g. just to buff for combat).
    #[test]
    fn opponent_target_is_optional() {
        let mut state = build_game(1, &[&[], &[]]);
        let mine = add(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        let theirs = add(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2 bystander
        let burrog = add(&mut state, PlayerId(0), BURROG_BARRAGE, Zone::Hand);
        for grp_id in [grp::FOREST, grp::FOREST] {
            add(&mut state, PlayerId(0), grp_id, Zone::Battlefield);
        }
        state.player_mut(PlayerId(0)).instants_sorceries_cast_this_turn = 1; // another I/S already cast
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let agent = TargetAgent { want_mine: mine, want_opp: None }; // decline the opponent creature
        let mut e = Engine::new(state, vec![Box::new(agent.clone()), Box::new(agent)]);

        e.cast_spell(PlayerId(0), burrog, CastVariant::Normal);
        e.resolve_top();
        e.run_agenda();

        assert_eq!(e.state.computed(mine).power, Some(3), "the +1/+0 still applies");
        assert_eq!(e.state.object(theirs).zone, Zone::Battlefield, "no target → no damage");
        assert_eq!(e.state.object(theirs).damage_marked, 0, "the bystander took nothing");
    }
}
