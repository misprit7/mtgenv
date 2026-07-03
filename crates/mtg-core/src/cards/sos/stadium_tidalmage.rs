//! Stadium Tidalmage — `{2}{U}{R}` Creature — Djinn Sorcerer 4/4 (first printed SOS).
//!
//! Oracle: "Whenever this creature enters or attacks, you may draw a card. If you do, discard a
//! card."
//!
//! **Fully implemented** — the "enters or attacks" trigger is encoded as two triggered abilities
//! (one `SelfEnters`, one `SelfAttacks`), each an optional loot: "you may draw a card. If you do,
//! discard a card." Modeled as `Optional{ Sequence[Draw 1, Discard 1] }` — declining draws nothing
//! and discards nothing; accepting draws then discards. Multicolored (U/R).

use crate::basics::Color;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const STADIUM_TIDALMAGE: u32 = 229;

/// "you may draw a card. If you do, discard a card." (the loot both triggers run).
fn loot() -> Effect {
    Effect::Optional {
        prompt: "Draw a card, then discard a card?".to_string(),
        body: Box::new(Effect::Sequence(vec![
            Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
            Effect::Discard { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
        ])),
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        STADIUM_TIDALMAGE,
        "Stadium Tidalmage",
        &[CreatureType::Djinn, CreatureType::Sorcerer],
        Color::Blue,
        mana_cost(2, &[(Color::Blue, 1), (Color::Red, 1)]),
        4,
        4,
        vec![
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: loot(),
            },
            Ability::Triggered {
                event: EventPattern::SelfAttacks,
                condition: None,
                intervening_if: false,
                effect: loot(),
            },
        ],
    );
    def.chars.colors = vec![Color::Blue, Color::Red];
    def.text = "Whenever this creature enters or attacks, you may draw a card. If you do, discard a card.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn stadium_tidalmage_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(STADIUM_TIDALMAGE).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Blue, Color::Red]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Optional {
                        prompt: "Draw a card, then discard a card?",
                        body: Sequence(
                            [
                                Draw {
                                    who: Controller,
                                    count: Fixed(
                                        1,
                                    ),
                                },
                                Discard {
                                    who: Controller,
                                    count: Fixed(
                                        1,
                                    ),
                                },
                            ],
                        ),
                    },
                },
                Triggered {
                    event: SelfAttacks,
                    condition: None,
                    intervening_if: false,
                    effect: Optional {
                        prompt: "Draw a card, then discard a card?",
                        body: Sequence(
                            [
                                Draw {
                                    who: Controller,
                                    count: Fixed(
                                        1,
                                    ),
                                },
                                Discard {
                                    who: Controller,
                                    count: Fixed(
                                        1,
                                    ),
                                },
                            ],
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: accepting the enters-loot draws a card then discards one (net-zero hand, one card
    /// cycled into the graveyard).
    #[test]
    fn stadium_tidalmage_loots_on_enter() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        // Accepts the "may" and discards the first card.
        #[derive(Clone)]
        struct LootAgent;
        impl Agent for LootAgent {
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

        let mut state = build_game(1, &[&[grp::FOREST], &[]]); // one card to draw
        let chars = state.card_db().get(STADIUM_TIDALMAGE).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let etb = match &state.card_db().get(STADIUM_TIDALMAGE).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected ETB Triggered, got {o:?}"),
        };
        let mut e = Engine::new(state, vec![Box::new(LootAgent), Box::new(LootAgent)]);
        let gy_before = e.state.players[0].graveyard.len();
        e.resolve_effect(
            &etb,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.players[0].graveyard.len(), gy_before + 1, "one card looted into the graveyard");
        assert!(e.state.players[0].hand.is_empty(), "drew one and discarded one → hand net-zero");
    }
}
