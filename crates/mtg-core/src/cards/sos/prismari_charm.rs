//! Prismari Charm — `{U}{R}` Instant (first printed SOS).
//!
//! Oracle: "Choose one — • Surveil 2, then draw a card. • Prismari Charm deals 1 damage to each of
//! one or two targets. • Return target nonland permanent to its owner's hand."
//!
//! **Fully implemented** — a three-mode `Modal` (choose one, CR 700.2):
//! 1. `Surveil 2` (S1) then `Draw 1`.
//! 2. `ForEachTarget { slot: 1–2 "any target", body: DealDamage 1 to Each }` — the variable-target-each
//!    cap, now binding **players too** (`Each` carries any `Target`), so it deals 1 to each of the one
//!    or two chosen targets (creatures or players).
//! 3. `MoveZone` a target nonland permanent to its owner's hand.

use crate::basics::{CardType, Color, DamageKind, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, Mode};

/// grp id (per-set ids live near their cards).
pub const PRISMARI_CHARM: u32 = 347;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "Surveil 2, then draw a card".to_string(),
                effect: Effect::Sequence(vec![
                    Effect::Surveil { count: ValueExpr::Fixed(2) },
                    Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
                ]),
            },
            Mode {
                label: "Deal 1 damage to each of one or two targets".to_string(),
                effect: Effect::ForEachTarget {
                    slot: TargetSpec { kind: TargetKind::Any, min: 1, max: 2, distinct: true },
                    body: Box::new(Effect::DealDamage {
                        amount: ValueExpr::Fixed(1),
                        to: EffectTarget::Each,
                        kind: DamageKind::Noncombat,
                    }),
                },
            },
            Mode {
                label: "Return target nonland permanent to its owner's hand".to_string(),
                effect: Effect::MoveZone {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Permanent(CardFilter::Not(Box::new(CardFilter::HasCardType(
                            CardType::Land,
                        )))),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                    to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
                },
            },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    let mut def = spell(
        PRISMARI_CHARM,
        "Prismari Charm",
        CardType::Instant,
        Color::Blue,
        mana_cost(0, &[(Color::Blue, 1), (Color::Red, 1)]),
        effect,
    )
    .with_text("Choose one —\n• Surveil 2, then draw a card.\n• Prismari Charm deals 1 damage to each of one or two targets.\n• Return target nonland permanent to its owner's hand.");
    def.chars.colors = vec![Color::Blue, Color::Red];
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

    fn mode_effect(i: usize) -> Effect {
        let mut db = CardDb::default();
        register(&mut db);
        match db.get(PRISMARI_CHARM).unwrap().spell_effect() {
            Some(Effect::Modal { modes, .. }) => modes[i].effect.clone(),
            _ => panic!("expected Modal"),
        }
    }

    #[test]
    fn prismari_charm_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PRISMARI_CHARM).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert_eq!(def.chars.colors, vec![Color::Blue, Color::Red]);
        assert!(def.fully_implemented);
        expect![[r#"
            Modal {
                modes: [
                    Mode {
                        label: "Surveil 2, then draw a card",
                        effect: Sequence(
                            [
                                Surveil {
                                    count: Fixed(
                                        2,
                                    ),
                                },
                                Draw {
                                    who: Controller,
                                    count: Fixed(
                                        1,
                                    ),
                                },
                            ],
                        ),
                    },
                    Mode {
                        label: "Deal 1 damage to each of one or two targets",
                        effect: ForEachTarget {
                            slot: TargetSpec {
                                kind: Any,
                                min: 1,
                                max: 2,
                                distinct: true,
                            },
                            body: DealDamage {
                                amount: Fixed(
                                    1,
                                ),
                                to: Each,
                                kind: Noncombat,
                            },
                        },
                    },
                    Mode {
                        label: "Return target nonland permanent to its owner's hand",
                        effect: MoveZone {
                            what: Target(
                                TargetSpec {
                                    kind: Permanent(
                                        Not(
                                            HasCardType(
                                                Land,
                                            ),
                                        ),
                                    ),
                                    min: 1,
                                    max: 1,
                                    distinct: true,
                                },
                            ),
                            to: ZoneDest {
                                zone: Hand,
                                pos: Any,
                            },
                        },
                    },
                ],
                min: 1,
                max: 1,
                allow_repeat: false,
            }"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Mode 2 deals 1 to EACH of two chosen creature targets.
    #[test]
    fn mode2_damages_two_creatures() {
        let mut state = build_game(1, &[&[], &[]]);
        let a = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let b = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let eff = mode_effect(1);
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &eff,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(a), Target::Object(b)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(a).damage_marked, 1, "first target took 1");
        assert_eq!(e.state.object(b).damage_marked, 1, "second target took 1");
    }

    /// Mode 2 can hit a PLAYER as one of its "any targets" — proving `ForEachTarget` binds a player to
    /// `Each` (the generalization from object-only). The player loses 1 life; the creature takes 1.
    #[test]
    fn mode2_can_damage_a_player() {
        let mut state = build_game(1, &[&[], &[]]);
        let c = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let eff = mode_effect(1);
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let life_before = e.state.player(PlayerId(1)).life;
        e.resolve_effect(
            &eff,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Player(PlayerId(1)), Target::Object(c)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(1)).life, life_before - 1, "player took 1 damage");
        assert_eq!(e.state.object(c).damage_marked, 1, "creature took 1 damage");
    }

    /// Mode 3 returns a nonland permanent to its owner's hand.
    #[test]
    fn mode3_bounces_a_permanent() {
        let mut state = build_game(1, &[&[], &[]]);
        let creature = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let eff = mode_effect(2);
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &eff,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(creature)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.player(PlayerId(1)).hand.contains(&creature), "bounced to owner's hand");
        assert!(!e.state.player(PlayerId(1)).battlefield.contains(&creature), "left the battlefield");
    }
}
