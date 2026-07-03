//! Follow the Lumarets — `{1}{G}` Sorcery (first printed SOS).
//!
//! Oracle: "Infusion — Look at the top four cards of your library. You may reveal a creature or land
//! card from among them and put it into your hand. If you gained life this turn, you may instead
//! reveal two creature and/or land cards from among them and put them into your hand. Put the rest on
//! the bottom of your library in a random order."
//!
//! **Fully implemented** — a filtered `LookAndPick` (look 4, take a creature-or-land, rest to the
//! bottom) whose `take` is gated by the Infusion condition `GainedLifeThisTurn`: two if you gained
//! life this turn, otherwise one. Mirrors Flow State's Conditional-take pattern. Simplification: the
//! remainder goes to the bottom in a fixed (not random) order — invisible, hidden-zone ordering.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::condition::Condition;
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const FOLLOW_THE_LUMARETS: u32 = 310;

fn look_take(take: i64) -> Effect {
    Effect::LookAndPick {
        count: ValueExpr::Fixed(4),
        take: ValueExpr::Fixed(take),
        take_to: Zone::Hand,
        rest_to: Zone::Library,
        // "a creature or land card".
        take_filter: CardFilter::AnyOf(vec![
            CardFilter::HasCardType(CardType::Creature),
            CardFilter::HasCardType(CardType::Land),
        ]),
    }
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Conditional {
        cond: Condition::GainedLifeThisTurn { who: PlayerRef::Controller },
        then: Box::new(look_take(2)),
        otherwise: Some(Box::new(look_take(1))),
    };
    db.insert(
        spell(
            FOLLOW_THE_LUMARETS,
            "Follow the Lumarets",
            CardType::Sorcery,
            Color::Green,
            mana_cost(1, &[(Color::Green, 1)]),
            effect,
        )
        .with_text("Infusion — Look at the top four cards of your library. You may reveal a creature or land card from among them and put it into your hand. If you gained life this turn, you may instead reveal two creature and/or land cards from among them and put them into your hand. Put the rest on the bottom of your library in a random order."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn follow_the_lumarets_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(FOLLOW_THE_LUMARETS).unwrap();
        assert!(def.fully_implemented);
        expect![[r#"
            Conditional {
                cond: GainedLifeThisTurn {
                    who: Controller,
                },
                then: LookAndPick {
                    count: Fixed(
                        4,
                    ),
                    take: Fixed(
                        2,
                    ),
                    take_to: Hand,
                    rest_to: Library,
                    take_filter: AnyOf(
                        [
                            HasCardType(
                                Creature,
                            ),
                            HasCardType(
                                Land,
                            ),
                        ],
                    ),
                },
                otherwise: Some(
                    LookAndPick {
                        count: Fixed(
                            4,
                        ),
                        take: Fixed(
                            1,
                        ),
                        take_to: Hand,
                        rest_to: Library,
                        take_filter: AnyOf(
                            [
                                HasCardType(
                                    Creature,
                                ),
                                HasCardType(
                                    Land,
                                ),
                            ],
                        ),
                    },
                ),
            }"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Behaviour: with no life gained this turn keep one creature/land of four; after gaining life,
    /// keep two. A nonland-noncreature card (an instant) is never offered.
    #[test]
    fn follow_the_lumarets_take_scales_with_infusion() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        #[derive(Clone)]
        struct KeepMax;
        impl Agent for KeepMax {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { max, .. } => DecisionResponse::Indices((0..*max).collect()),
                    _ => DecisionResponse::Pass,
                }
            }
        }
        // Top four are all creature/land (takeable); one extra instant sits deeper (never seen).
        let kept = |gained_life: bool| {
            let lib = vec![grp::LIGHTNING_BOLT, grp::FOREST, grp::GRIZZLY_BEARS, grp::FOREST, grp::GRIZZLY_BEARS];
            let mut state = build_game(1, &[&lib, &[]]);
            if gained_life {
                state.player_mut(PlayerId(0)).life_gained_this_turn = 1;
            }
            let effect = state.card_db().get(FOLLOW_THE_LUMARETS).unwrap().spell_effect().unwrap().clone();
            let mut e = Engine::new(state, vec![Box::new(KeepMax), Box::new(KeepMax)]);
            let hand0 = e.state.players[0].hand.len();
            e.resolve_effect(&effect, &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() }, WbReason::Resolve(StackId(0)));
            e.state.players[0].hand.len() - hand0
        };
        assert_eq!(kept(false), 1, "no life gained → take one");
        assert_eq!(kept(true), 2, "gained life this turn → take two");
    }
}
