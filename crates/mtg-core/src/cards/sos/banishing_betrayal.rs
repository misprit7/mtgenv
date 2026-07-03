//! Banishing Betrayal — `{1}{U}` Instant (first printed SOS).
//!
//! Oracle: "Return target nonland permanent to its owner's hand. Surveil 1."
//!
//! **Fully implemented** — a single-target `MoveZone` bounce of a nonland permanent to its owner's
//! hand, then `Surveil 1`.

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const BANISHING_BETRAYAL: u32 = 234;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::MoveZone {
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
        Effect::Surveil { count: ValueExpr::Fixed(1) },
    ]);
    db.insert(
        spell(
            BANISHING_BETRAYAL,
            "Banishing Betrayal",
            CardType::Instant,
            Color::Blue,
            mana_cost(1, &[(Color::Blue, 1)]),
            effect,
        )
        .with_text("Return target nonland permanent to its owner's hand. Surveil 1."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn banishing_betrayal_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BANISHING_BETRAYAL).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            Sequence(
                [
                    MoveZone {
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
                    Surveil {
                        count: Fixed(
                            1,
                        ),
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: bounces the opponent's creature back to its owner's hand.
    #[test]
    fn banishing_betrayal_bounces_the_target() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        #[derive(Clone)]
        struct KeepAll;
        impl Agent for KeepAll {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { .. } => DecisionResponse::Indices(vec![]),
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = build_game(1, &[&[grp::FOREST], &[]]);
        let bears = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let victim = state.add_card(PlayerId(1), bears, Zone::Battlefield);
        let effect = state.card_db().get(BANISHING_BETRAYAL).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(KeepAll), Box::new(KeepAll)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(victim)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.players[1].hand.contains(&victim), "the nonland permanent returned to its owner's hand");
        assert!(!e.state.players[1].battlefield.contains(&victim), "and left the battlefield");
    }
}
