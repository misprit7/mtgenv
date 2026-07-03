//! Harsh Annotation — `{1}{W}` Instant (first printed SOS).
//!
//! Oracle: "Destroy target creature. Its controller creates a 1/1 white and black Inkling creature
//! token with flying."
//!
//! **Fully implemented** — `Destroy` the one declared target creature, then the **destroyed
//! creature's controller** (`ControllerOfTarget(0)`, snapshotted before the Destroy moves it)
//! creates the shared Inkling token.

use crate::basics::{CardType, Color};
use crate::cards::helpers::inkling_token;
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const HARSH_ANNOTATION: u32 = 220;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Destroy {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
        Effect::CreateToken {
            spec: inkling_token(),
            count: ValueExpr::Fixed(1),
            controller: PlayerRef::ControllerOfTarget(0),
        },
    ]);
    db.insert(
        spell(
            HARSH_ANNOTATION,
            "Harsh Annotation",
            CardType::Instant,
            Color::White,
            mana_cost(1, &[(Color::White, 1)]),
            effect,
        )
        .with_text("Destroy target creature. Its controller creates a 1/1 white and black Inkling creature token with flying."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn harsh_annotation_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(HARSH_ANNOTATION).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    Destroy {
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
                    },
                    CreateToken {
                        spec: TokenSpec {
                            name: "Inkling",
                            card_types: [
                                Creature,
                            ],
                            subtypes: [
                                Creature(
                                    Inkling,
                                ),
                            ],
                            colors: [
                                White,
                                Black,
                            ],
                            power: 1,
                            toughness: 1,
                            keywords: [
                                Flying,
                            ],
                            counters: [],
                        },
                        count: Fixed(
                            1,
                        ),
                        controller: ControllerOfTarget(
                            0,
                        ),
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: destroys the opponent's creature and the **opponent** (its controller) gets the Inkling.
    #[test]
    fn harsh_annotation_destroys_and_opponent_gets_inkling() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let victim = state.add_card(PlayerId(1), bears, Zone::Battlefield);
        let effect = state.card_db().get(HARSH_ANNOTATION).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        let p1_before = e.state.players[1].battlefield.len();
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(victim)],
                target_controllers: vec![Some(PlayerId(1))],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.players[1].graveyard.contains(&victim), "victim destroyed");
        // P1's battlefield: lost the Bears (−1), gained the Inkling (+1) → net unchanged count.
        assert_eq!(e.state.players[1].battlefield.len(), p1_before, "opponent got a token (net count same)");
        assert!(
            e.state.players[1].battlefield.iter().any(|&o| e.state.object(o).chars.name == "Inkling"),
            "the Inkling is on the opponent's battlefield"
        );
        assert!(e.state.players[0].battlefield.is_empty(), "the caster gets no token");
    }
}
