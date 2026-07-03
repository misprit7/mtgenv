//! Muse Seeker — `{1}{U}` Creature — Elf Wizard 1/2 (first printed SOS).
//!
//! Oracle: "Opus — Whenever you cast an instant or sorcery spell, draw a card. Then discard a card
//! unless five or more mana was spent to cast that spell."
//!
//! **Fully implemented** — an Opus cast-trigger: `Draw 1`, then a `Conditional` on
//! `Not(ManaSpentOnTrigger ≥ 5)` that discards a card (so a big spell lets you keep the draw).

use crate::basics::Color;
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const MUSE_SEEKER: u32 = 257;

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            MUSE_SEEKER,
            "Muse Seeker",
            &[CreatureType::Elf, CreatureType::Wizard],
            Color::Blue,
            mana_cost(1, &[(Color::Blue, 1)]),
            1,
            2,
            vec![Ability::Triggered {
                event: EventPattern::SpellCast(instant_or_sorcery()),
                condition: None,
                intervening_if: false,
                effect: Effect::Sequence(vec![
                    Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
                    Effect::Conditional {
                        cond: Condition::Not(Box::new(Condition::ValueAtLeast(
                            ValueExpr::ManaSpentOnTrigger,
                            ValueExpr::Fixed(5),
                        ))),
                        then: Box::new(Effect::Discard {
                            who: PlayerRef::Controller,
                            count: ValueExpr::Fixed(1),
                        }),
                        otherwise: None,
                    },
                ]),
            }],
        )
        .with_text("Opus — Whenever you cast an instant or sorcery spell, draw a card. Then discard a card unless five or more mana was spent to cast that spell."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn muse_seeker_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MUSE_SEEKER).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SpellCast(
                        AnyOf(
                            [
                                HasCardType(
                                    Instant,
                                ),
                                HasCardType(
                                    Sorcery,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: Sequence(
                        [
                            Draw {
                                who: Controller,
                                count: Fixed(
                                    1,
                                ),
                            },
                            Conditional {
                                cond: Not(
                                    ValueAtLeast(
                                        ManaSpentOnTrigger,
                                        Fixed(
                                            5,
                                        ),
                                    ),
                                ),
                                then: Discard {
                                    who: Controller,
                                    count: Fixed(
                                        1,
                                    ),
                                },
                                otherwise: None,
                            },
                        ],
                    ),
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: cheap Opus loots (draw then discard, net-zero hand); a 5-mana spell keeps the draw.
    #[test]
    fn muse_seeker_opus_keeps_draw_on_big_spell() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        #[derive(Clone)]
        struct DiscardAgent;
        impl Agent for DiscardAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { min, max, from, .. } => {
                        let n = (*min).max(1).min(*max).min(from.len() as u32);
                        DecisionResponse::Indices((0..n).collect())
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let hand_after = |mana_spent: u32| {
            // Library of one card to draw.
            let mut state = build_game(1, &[&[grp::FOREST], &[]]);
            let src = {
                let c = state.card_db().get(MUSE_SEEKER).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            let spell = {
                let c = state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Stack)
            };
            state.objects.get_mut(&spell).unwrap().mana_spent = mana_spent;
            let etb = match &state.card_db().get(MUSE_SEEKER).unwrap().abilities[0] {
                Ability::Triggered { effect, .. } => effect.clone(),
                o => panic!("expected Opus Triggered, got {o:?}"),
            };
            let mut e = Engine::new(state, vec![Box::new(DiscardAgent), Box::new(DiscardAgent)]);
            e.resolve_effect(
                &etb,
                &ResolutionCtx {
                    controller: Some(PlayerId(0)),
                    source: Some(src),
                    triggering_spell: Some(spell),
                    ..Default::default()
                },
                WbReason::Resolve(StackId(0)),
            );
            e.state.players[0].hand.len()
        };
        assert_eq!(hand_after(3), 0, "cheap → drew one, discarded one → net zero");
        assert_eq!(hand_after(5), 1, "5+ mana → kept the drawn card");
    }
}
