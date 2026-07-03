//! Transcendent Archaic — `{7}` Creature — Avatar 6/6 (first printed SOS).
//!
//! Oracle: "Vigilance / Converge — When this creature enters, you may draw X cards, where X is the
//! number of colors of mana spent to cast this creature. If you draw one or more cards this way,
//! discard two cards."
//!
//! **Fully implemented** — printed Vigilance + an ETB `Optional{ IfYouDo{ draw `ColorsSpent`, discard
//! two } }`: the `X` is the Converge count recorded at cast, read at ETB like `ManaSpent`.

use crate::basics::Color;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const TRANSCENDENT_ARCHAIC: u32 = 294;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        TRANSCENDENT_ARCHAIC,
        "Transcendent Archaic",
        &[CreatureType::Avatar],
        Color::Colorless,
        mana_cost(7, &[]),
        6,
        6,
        vec![Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            effect: Effect::Optional {
                prompt: "Draw X cards (then discard two)?".to_string(),
                body: Box::new(Effect::IfYouDo {
                    cost: Box::new(Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::ColorsSpent }),
                    reward: Box::new(Effect::Discard { who: PlayerRef::Controller, count: ValueExpr::Fixed(2) }),
                }),
            },
        }],
    );
    def.chars.colors = vec![];
    def.chars.keywords = vec![Keyword::Vigilance];
    def.text = "Vigilance\nConverge — When this creature enters, you may draw X cards, where X is the number of colors of mana spent to cast this creature. If you draw one or more cards this way, discard two cards.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn transcendent_archaic_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TRANSCENDENT_ARCHAIC).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Vigilance]);
        assert!(def.chars.colors.is_empty());
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Optional {
                        prompt: "Draw X cards (then discard two)?",
                        body: IfYouDo {
                            cost: Draw {
                                who: Controller,
                                count: ColorsSpent,
                            },
                            reward: Discard {
                                who: Controller,
                                count: Fixed(
                                    2,
                                ),
                            },
                        },
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: with colors_spent = 2 and accepting the "may", draw 2 then discard 2 (library -2,
    /// two cards land in the graveyard).
    #[test]
    fn transcendent_archaic_draws_x_then_discards_two() {
        use crate::agent::{Agent, ConfirmKind, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        #[derive(Clone)] struct Accept;
        impl Agent for Accept {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::Confirm { kind: ConfirmKind::MayEffect } => DecisionResponse::Bool(true),
                    DecisionRequest::SelectCards { min, .. } => DecisionResponse::Indices((0..*min).collect()),
                    _ => DecisionResponse::Pass,
                }
            }
        }
        let lib: Vec<u32> = std::iter::repeat(grp::FOREST).take(4).collect();
        let mut state = build_game(1, &[&lib, &[]]);
        let src = state.add_card(PlayerId(0), state.card_db().get(TRANSCENDENT_ARCHAIC).unwrap().chars.clone(), Zone::Battlefield);
        state.objects.get_mut(&src).unwrap().colors_spent = 2;
        let eff = match &state.card_db().get(TRANSCENDENT_ARCHAIC).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
        let mut e = Engine::new(state, vec![Box::new(Accept), Box::new(Accept)]);
        let (lib0, gy0) = (e.state.players[0].library.len(), e.state.players[0].graveyard.len());
        e.resolve_effect(&eff, &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.players[0].library.len(), lib0 - 2, "drew X=2");
        assert_eq!(e.state.players[0].graveyard.len(), gy0 + 2, "discarded two");
    }
}
