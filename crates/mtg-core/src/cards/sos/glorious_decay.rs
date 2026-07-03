//! Glorious Decay — `{1}{G}` Instant (first printed SOS).
//!
//! Oracle: "Choose one —
//!   • Destroy target artifact.
//!   • Glorious Decay deals 4 damage to target creature with flying.
//!   • Exile target card from a graveyard. Draw a card."
//!
//! **Fully implemented** — a `Modal` "choose one" over three wired effects: `Destroy` (target
//! artifact), `DealDamage 4` to a "target creature **with flying**" (the new `CardFilter::HasKeyword`
//! cap, landed alongside this card), and `Exile` a graveyard card + `Draw`.

use crate::basics::{CardType, Color, DamageKind, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::Keyword;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, Mode};

/// grp id (per-set ids live near their cards).
pub const GLORIOUS_DECAY: u32 = 324;

fn one_target(kind: TargetKind) -> EffectTarget {
    EffectTarget::Target(TargetSpec { kind, min: 1, max: 1, distinct: true })
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "Destroy target artifact".to_string(),
                effect: Effect::Destroy {
                    what: one_target(TargetKind::Permanent(CardFilter::HasCardType(CardType::Artifact))),
                },
            },
            Mode {
                label: "Deal 4 damage to target creature with flying".to_string(),
                effect: Effect::DealDamage {
                    amount: ValueExpr::Fixed(4),
                    to: one_target(TargetKind::Creature(CardFilter::HasKeyword(Keyword::Flying))),
                    kind: DamageKind::Noncombat,
                },
            },
            Mode {
                label: "Exile target card from a graveyard. Draw a card".to_string(),
                effect: Effect::Sequence(vec![
                    Effect::Exile {
                        what: one_target(TargetKind::CardInZone {
                            zone: Zone::Graveyard,
                            filter: CardFilter::Any,
                        }),
                    },
                    Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
                ]),
            },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    db.insert(
        spell(GLORIOUS_DECAY, "Glorious Decay", CardType::Instant, Color::Green, mana_cost(1, &[(Color::Green, 1)]), effect)
            .with_text("Choose one —\n• Destroy target artifact.\n• Glorious Decay deals 4 damage to target creature with flying.\n• Exile target card from a graveyard. Draw a card."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn glorious_decay_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(GLORIOUS_DECAY).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert_eq!(def.chars.colors, vec![Color::Green]);
        assert_eq!(def.chars.mana_value(), 2);
        assert!(def.fully_implemented);
    }

    #[test]
    fn glorious_decay_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(GLORIOUS_DECAY).unwrap();
        expect![[r#"
            Modal {
                modes: [
                    Mode {
                        label: "Destroy target artifact",
                        effect: Destroy {
                            what: Target(
                                TargetSpec {
                                    kind: Permanent(
                                        HasCardType(
                                            Artifact,
                                        ),
                                    ),
                                    min: 1,
                                    max: 1,
                                    distinct: true,
                                },
                            ),
                        },
                    },
                    Mode {
                        label: "Deal 4 damage to target creature with flying",
                        effect: DealDamage {
                            amount: Fixed(
                                4,
                            ),
                            to: Target(
                                TargetSpec {
                                    kind: Creature(
                                        HasKeyword(
                                            Flying,
                                        ),
                                    ),
                                    min: 1,
                                    max: 1,
                                    distinct: true,
                                },
                            ),
                            kind: Noncombat,
                        },
                    },
                    Mode {
                        label: "Exile target card from a graveyard. Draw a card",
                        effect: Sequence(
                            [
                                Exile {
                                    what: Target(
                                        TargetSpec {
                                            kind: CardInZone {
                                                zone: Graveyard,
                                                filter: Any,
                                            },
                                            min: 1,
                                            max: 1,
                                            distinct: true,
                                        },
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
                ],
                min: 1,
                max: 1,
                allow_repeat: false,
            }"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// The `HasKeyword(Flying)` filter: the flying-damage mode is legal only when a flying creature
    /// exists, and resolving it deals 4 to that creature.
    #[test]
    fn glorious_decay_flying_mode_targets_only_flyers() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        let flying_mode = |db: &crate::cards::CardDb| match db.get(GLORIOUS_DECAY).unwrap().spell_effect() {
            Some(Effect::Modal { modes, .. }) => modes[1].clone(),
            _ => panic!("expected Modal"),
        };

        // A vanilla (non-flying) creature: the flying-damage mode has no legal target.
        let mut state = build_game(1, &[&[], &[]]);
        let _vanilla = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Battlefield)
        };
        let mode = flying_mode(&state.card_db());
        let e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        assert!(!e.mode_is_legal(&mode, PlayerId(0)), "no flyer → flying-damage mode illegal");

        // Give a creature Flying: the mode is legal, and resolving it deals 4 damage.
        let mut state = build_game(1, &[&[], &[]]);
        let flyer = {
            let mut c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            c.keywords.push(Keyword::Flying);
            state.add_card(PlayerId(1), c, Zone::Battlefield)
        };
        let mode = flying_mode(&state.card_db());
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        assert!(e.mode_is_legal(&mode, PlayerId(0)), "a flyer → flying-damage mode legal");
        let eff = mode.effect.clone();
        e.resolve_effect(
            &eff,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(flyer)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(flyer).damage_marked, 4, "4 damage dealt to the flyer");
    }
}
