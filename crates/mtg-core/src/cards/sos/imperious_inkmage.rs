//! Imperious Inkmage — `{1}{W}{B}` Creature — Orc Warlock 3/3 (first printed SOS).
//!
//! Oracle: "Vigilance / When this creature enters, surveil 2."
//!
//! **Fully implemented** — printed Vigilance plus an ETB `Surveil 2` (look at the top two of your
//! library, bin any number, keep the rest on top). Multicolored (W/B).

use crate::basics::Color;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::value::ValueExpr;
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const IMPERIOUS_INKMAGE: u32 = 232;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        IMPERIOUS_INKMAGE,
        "Imperious Inkmage",
        &[CreatureType::Orc, CreatureType::Warlock],
        Color::White,
        mana_cost(1, &[(Color::White, 1), (Color::Black, 1)]),
        3,
        3,
        vec![Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            effect: Effect::Surveil { count: ValueExpr::Fixed(2) },
        }],
    );
    def.chars.colors = vec![Color::White, Color::Black];
    def.chars.keywords = vec![Keyword::Vigilance];
    def.text = "Vigilance\nWhen this creature enters, surveil 2.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn imperious_inkmage_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(IMPERIOUS_INKMAGE).unwrap();
        assert_eq!(def.chars.colors, vec![Color::White, Color::Black]);
        assert_eq!(def.chars.keywords, vec![Keyword::Vigilance]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Surveil {
                        count: Fixed(
                            2,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: surveil 2 with an agent that bins both top cards → both go to the graveyard.
    #[test]
    fn imperious_inkmage_surveils_two_to_graveyard() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        // Bins every card offered by a surveil.
        #[derive(Clone)]
        struct BinAll;
        impl Agent for BinAll {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { from, .. } => {
                        DecisionResponse::Indices((0..from.len() as u32).collect())
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        // Library of three Forests (top two get surveiled).
        let mut state = build_game(1, &[&[grp::FOREST, grp::FOREST, grp::FOREST], &[]]);
        let chars = state.card_db().get(IMPERIOUS_INKMAGE).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let etb = match &state.card_db().get(IMPERIOUS_INKMAGE).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            o => panic!("expected ETB Triggered, got {o:?}"),
        };
        let mut e = Engine::new(state, vec![Box::new(BinAll), Box::new(BinAll)]);
        let lib_before = e.state.players[0].library.len();
        e.resolve_effect(
            &etb,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.players[0].graveyard.len(), 2, "both surveiled cards went to the graveyard");
        assert_eq!(e.state.players[0].library.len(), lib_before - 2, "and left the library");
    }
}
