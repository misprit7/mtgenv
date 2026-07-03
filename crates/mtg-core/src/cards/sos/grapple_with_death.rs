//! Grapple with Death — `{1}{B}{G}` Sorcery (first printed SOS).
//!
//! Oracle: "Destroy target artifact or creature. You gain 1 life."
//!
//! **Fully implemented** — a `Destroy` over one declared target ("artifact or creature") followed
//! by `GainLife 1` for the caster. Multicolored (B/G): built via the `spell` helper (which takes a
//! single colour) then the colour vector is corrected to `[Black, Green]` (CR 105.2 / 202.2).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const GRAPPLE_WITH_DEATH: u32 = 202;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Destroy {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Permanent(CardFilter::AnyOf(vec![
                    CardFilter::HasCardType(CardType::Artifact),
                    CardFilter::HasCardType(CardType::Creature),
                ])),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
        Effect::GainLife {
            who: PlayerRef::Controller,
            amount: ValueExpr::Fixed(1),
        },
    ]);
    let mut def = spell(
        GRAPPLE_WITH_DEATH,
        "Grapple with Death",
        CardType::Sorcery,
        Color::Black,
        mana_cost(1, &[(Color::Black, 1), (Color::Green, 1)]),
        effect,
    )
    .with_text("Destroy target artifact or creature. You gain 1 life.");
    def.chars.colors = vec![Color::Black, Color::Green];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn grapple_with_death_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(GRAPPLE_WITH_DEATH).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.colors, vec![Color::Black, Color::Green]);
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    Destroy {
                        what: Target(
                            TargetSpec {
                                kind: Permanent(
                                    AnyOf(
                                        [
                                            HasCardType(
                                                Artifact,
                                            ),
                                            HasCardType(
                                                Creature,
                                            ),
                                        ],
                                    ),
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                    },
                    GainLife {
                        who: Controller,
                        amount: Fixed(
                            1,
                        ),
                    },
                ],
            )"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: resolving Grapple with Death destroys the targeted creature (→ owner's graveyard)
    /// and the caster gains 1 life.
    #[test]
    fn grapple_destroys_and_gains_life() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let victim = state.add_card(PlayerId(1), bears, Zone::Battlefield);
        let effect = state.card_db().get(GRAPPLE_WITH_DEATH).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let life_before = e.state.player(PlayerId(0)).life;
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(victim)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.players[1].graveyard.contains(&victim), "destroyed → owner's graveyard");
        assert_eq!(e.state.player(PlayerId(0)).life, life_before + 1, "caster gains 1 life");
    }
}
