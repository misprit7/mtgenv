//! Heated Argument — `{4}{R}` Instant (first printed SOS).
//!
//! Oracle: "Heated Argument deals 6 damage to target creature. You may exile a card from your
//! graveyard. If you do, Heated Argument also deals 2 damage to that creature's controller."
//!
//! **Fully implemented** — `DealDamage 6` to a target creature, then an `Optional{ IfYouDo{ … } }`:
//! the cost is "exile a card from your graveyard" (a resolution-time **`Select`** exile — the S-cap
//! wired alongside this card so `Effect::Exile` reports its performed flag), and the reward deals 2
//! damage to *that creature's controller* (`PlayerRef::ControllerOfTarget(0)`, snapshotted at
//! resolution start). Declining, or an empty graveyard (the select can't reach its `min`), withholds
//! the 2 damage.

use crate::basics::{CardType, Color, DamageKind, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const HEATED_ARGUMENT: u32 = 322;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::DealDamage {
            amount: ValueExpr::Fixed(6),
            to: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
            kind: DamageKind::Noncombat,
        },
        // "You may exile a card from your graveyard. If you do, … 2 damage to that creature's controller."
        Effect::Optional {
            prompt: "Exile a card from your graveyard to deal 2 damage to that creature's controller?"
                .to_string(),
            body: Box::new(Effect::IfYouDo {
                cost: Box::new(Effect::Exile {
                    what: EffectTarget::Select(SelectSpec {
                        zone: Zone::Graveyard,
                        filter: CardFilter::Any,
                        chooser: PlayerRef::Controller,
                        min: ValueExpr::Fixed(1),
                        max: ValueExpr::Fixed(1),
                    }),
                }),
                reward: Box::new(Effect::DealDamage {
                    amount: ValueExpr::Fixed(2),
                    to: EffectTarget::Player(PlayerRef::ControllerOfTarget(0)),
                    kind: DamageKind::Noncombat,
                }),
            }),
        },
    ]);
    db.insert(
        spell(
            HEATED_ARGUMENT,
            "Heated Argument",
            CardType::Instant,
            Color::Red,
            mana_cost(4, &[(Color::Red, 1)]),
            effect,
        )
        .with_text("Heated Argument deals 6 damage to target creature. You may exile a card from your graveyard. If you do, Heated Argument also deals 2 damage to that creature's controller."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn heated_argument_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(HEATED_ARGUMENT).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
        assert_eq!(def.chars.colors, vec![Color::Red]);
        assert_eq!(def.chars.mana_value(), 5);
        assert!(def.fully_implemented);
    }

    #[test]
    fn heated_argument_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(HEATED_ARGUMENT).unwrap();
        expect![[r#"
            Sequence(
                [
                    DealDamage {
                        amount: Fixed(
                            6,
                        ),
                        to: Target(
                            TargetSpec {
                                kind: Creature(
                                    Any,
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        kind: Noncombat,
                    },
                    Optional {
                        prompt: "Exile a card from your graveyard to deal 2 damage to that creature's controller?",
                        body: IfYouDo {
                            cost: Exile {
                                what: Select(
                                    SelectSpec {
                                        zone: Graveyard,
                                        filter: Any,
                                        chooser: Controller,
                                        min: Fixed(
                                            1,
                                        ),
                                        max: Fixed(
                                            1,
                                        ),
                                    },
                                ),
                            },
                            reward: DealDamage {
                                amount: Fixed(
                                    2,
                                ),
                                to: Player(
                                    ControllerOfTarget(
                                        0,
                                    ),
                                ),
                                kind: Noncombat,
                            },
                        },
                    },
                ],
            )"#]].assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: 6 damage to the creature always; exiling a graveyard card adds 2 damage to that
    /// creature's controller. An empty graveyard (the select can't reach its min) withholds the 2.
    #[test]
    fn heated_argument_optional_graveyard_exile_gates_the_bonus() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        #[derive(Clone)]
        struct YesAgent;
        impl Agent for YesAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                    DecisionRequest::SelectCards { min, max, from, .. } => {
                        let n = (*min).max(1).min(*max).min(from.len() as u32);
                        DecisionResponse::Indices((0..n).collect())
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        // `gy_cards` = how many cards P0 has to exile. Returns (P1 life, was a card exiled).
        let run = |gy_cards: usize| -> (i32, bool) {
            use crate::basics::Target;
            let mut state = build_game(1, &[&[], &[]]);
            // P1's creature is the target; P0 owns the graveyard we exile from.
            let victim = {
                let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
                state.add_card(PlayerId(1), c, Zone::Battlefield)
            };
            for _ in 0..gy_cards {
                let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Graveyard);
            }
            let gy_before = state.player(PlayerId(0)).graveyard.len();
            let effect = state.card_db().get(HEATED_ARGUMENT).unwrap().spell_effect().unwrap().clone();
            let mut e = Engine::new(state, vec![Box::new(YesAgent), Box::new(YesAgent)]);
            e.resolve_effect(
                &effect,
                &ResolutionCtx {
                    controller: Some(PlayerId(0)),
                    chosen_targets: vec![Target::Object(victim)],
                    // The real cast path snapshots each target's controller at resolution start; a
                    // direct `resolve_effect` must supply it so `ControllerOfTarget(0)` = P1, not the
                    // caster (see Erode's test).
                    target_controllers: vec![Some(PlayerId(1))],
                    ..Default::default()
                },
                WbReason::Resolve(StackId(0)),
            );
            let exiled = e.state.player(PlayerId(0)).graveyard.len() < gy_before
                || !e.state.player(PlayerId(0)).exile.is_empty();
            (e.state.player(PlayerId(1)).life, exiled)
        };

        // With a card to exile: exile happens → the controller (P1) takes 2 (20 → 18).
        assert_eq!(run(1), (18, true), "exiling a gy card deals 2 to the creature's controller");
        // Empty graveyard: even saying yes, the exile can't reach min 1 → reward withheld, P1 at 20.
        assert_eq!(run(0), (20, false), "no gy card → no exile → no bonus damage");
    }
}
