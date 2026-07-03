//! Flow State — `{1}{U}` Sorcery (first printed SOS).
//!
//! Oracle: "Look at the top three cards of your library. Put one of them into your hand and the rest
//! on the bottom of your library in any order. If there is an instant card and a sorcery card in your
//! graveyard, instead put two of them into your hand and the rest on the bottom of your library in
//! any order."
//!
//! **Fully implemented** — a `LookAndPick` (look 3, rest to the bottom) whose `take` is gated by a
//! resolution-time `Conditional`: keep two if you have both an instant and a sorcery in your
//! graveyard, otherwise one.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::condition::Condition;
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const FLOW_STATE: u32 = 280;

fn look_take(take: i64) -> Effect {
    Effect::LookAndPick {
        count: ValueExpr::Fixed(3),
        take: ValueExpr::Fixed(take),
        take_to: Zone::Hand,
        rest_to: Zone::Library,
    }
}

pub fn register(db: &mut CardDb) {
    let has_instant_and_sorcery = Condition::All(vec![
        Condition::CountAtLeast {
            zone: Zone::Graveyard,
            filter: CardFilter::HasCardType(CardType::Instant),
            controller: Some(PlayerRef::Controller),
            n: ValueExpr::Fixed(1),
        },
        Condition::CountAtLeast {
            zone: Zone::Graveyard,
            filter: CardFilter::HasCardType(CardType::Sorcery),
            controller: Some(PlayerRef::Controller),
            n: ValueExpr::Fixed(1),
        },
    ]);
    let effect = Effect::Conditional {
        cond: has_instant_and_sorcery,
        then: Box::new(look_take(2)),
        otherwise: Some(Box::new(look_take(1))),
    };
    db.insert(
        spell(
            FLOW_STATE,
            "Flow State",
            CardType::Sorcery,
            Color::Blue,
            mana_cost(1, &[(Color::Blue, 1)]),
            effect,
        )
        .with_text("Look at the top three cards of your library. Put one of them into your hand and the rest on the bottom of your library in any order. If there is an instant card and a sorcery card in your graveyard, instead put two of them into your hand and the rest on the bottom of your library in any order."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn flow_state_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        assert!(db.get(FLOW_STATE).unwrap().fully_implemented);
        expect![[r#"
            Conditional {
                cond: All(
                    [
                        CountAtLeast {
                            zone: Graveyard,
                            filter: HasCardType(
                                Instant,
                            ),
                            controller: Some(
                                Controller,
                            ),
                            n: Fixed(
                                1,
                            ),
                        },
                        CountAtLeast {
                            zone: Graveyard,
                            filter: HasCardType(
                                Sorcery,
                            ),
                            controller: Some(
                                Controller,
                            ),
                            n: Fixed(
                                1,
                            ),
                        },
                    ],
                ),
                then: LookAndPick {
                    count: Fixed(
                        3,
                    ),
                    take: Fixed(
                        2,
                    ),
                    take_to: Hand,
                    rest_to: Library,
                },
                otherwise: Some(
                    LookAndPick {
                        count: Fixed(
                            3,
                        ),
                        take: Fixed(
                            1,
                        ),
                        take_to: Hand,
                        rest_to: Library,
                    },
                ),
            }"#]].assert_eq(&format!("{:#?}", db.get(FLOW_STATE).unwrap().spell_effect().unwrap()));
    }

    /// Behaviour: with no instant+sorcery in the graveyard, keep one of three (net library -1); with
    /// both present, keep two (net library -2).
    #[test]
    fn flow_state_take_scales_with_graveyard() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        #[derive(Clone)] struct KeepLowest;
        impl Agent for KeepLowest {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { min, .. } => DecisionResponse::Indices((0..*min).collect()),
                    _ => DecisionResponse::Pass,
                }
            }
        }
        let kept = |gy: &[u32]| {
            let lib = vec![grp::FOREST, grp::ISLAND, grp::MOUNTAIN, grp::FOREST];
            let mut state = build_game(1, &[&lib, &[]]);
            for &g in gy {
                let c = state.card_db().get(g).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Graveyard);
            }
            let effect = state.card_db().get(FLOW_STATE).unwrap().spell_effect().unwrap().clone();
            let mut e = Engine::new(state, vec![Box::new(KeepLowest), Box::new(KeepLowest)]);
            let hand0 = e.state.players[0].hand.len();
            e.resolve_effect(&effect, &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() }, WbReason::Resolve(StackId(0)));
            e.state.players[0].hand.len() - hand0
        };
        assert_eq!(kept(&[]), 1, "no instant+sorcery in gy → keep one");
        assert_eq!(kept(&[grp::LIGHTNING_BOLT, grp::DIVINATION]), 2, "instant + sorcery in gy → keep two");
    }
}
