//! Geometer's Arthropod — `{G}{U}` Creature — Fractal Crab 1/4 (first printed SOS).
//!
//! Oracle: "Whenever you cast a spell with {X} in its mana cost, look at the top X cards of your
//! library. Put one of them into your hand and the rest on the bottom of your library in a random
//! order."
//!
//! **Fully implemented** — a cast-with-{X} trigger (S21: `SpellCast(All([ControlledBy, HasXInCost]))`)
//! whose effect is a `LookAndPick` (S2) with `count = ValueExpr::XOfTriggeringSpell` — the new value
//! reading the triggering spell's chosen `{X}` (`Object.cast_x`, recorded at cast alongside
//! `mana_spent`). Take one to hand, the rest to the bottom of the library.

use crate::basics::{Color, Zone};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const GEOMETERS_ARTHROPOD: u32 = 345;

/// "a spell with {X} in its mana cost that you control" (CR 107.3) — the S21 cast filter.
fn your_x_spell() -> CardFilter {
    CardFilter::All(vec![
        CardFilter::ControlledBy(PlayerRef::Controller),
        CardFilter::HasXInCost,
    ])
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        GEOMETERS_ARTHROPOD,
        "Geometer's Arthropod",
        &[CreatureType::Fractal, CreatureType::Crab],
        Color::Green,
        mana_cost(0, &[(Color::Green, 1), (Color::Blue, 1)]),
        1,
        4,
        vec![Ability::Triggered {
            event: EventPattern::SpellCast(your_x_spell()),
            condition: None,
            intervening_if: false,
            effect: Effect::LookAndPick {
                count: ValueExpr::XOfTriggeringSpell,
                take: ValueExpr::Fixed(1),
                take_to: Zone::Hand,
                rest_to: Zone::Library,
                take_filter: CardFilter::Any,
            },
        }],
    );
    def.chars.colors = vec![Color::Green, Color::Blue];
    def.text = "Whenever you cast a spell with {X} in its mana cost, look at the top X cards of your library. Put one of them into your hand and the rest on the bottom of your library in a random order.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView, SelectReason};
    use crate::basics::Zone;
    use crate::cards::{grp, starter_db};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use expect_test::expect;
    use std::sync::Arc;

    #[test]
    fn geometers_arthropod_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(GEOMETERS_ARTHROPOD).unwrap();
        assert_eq!((def.chars.power, def.chars.toughness), (Some(1), Some(4)));
        assert_eq!(def.chars.colors, vec![Color::Green, Color::Blue]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SpellCast(
                        All(
                            [
                                ControlledBy(
                                    Controller,
                                ),
                                HasXInCost,
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: LookAndPick {
                        count: XOfTriggeringSpell,
                        take: Fixed(
                            1,
                        ),
                        take_to: Hand,
                        rest_to: Library,
                        take_filter: Any,
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }

    /// An agent that keeps the first card offered in a look-and-pick and passes otherwise.
    #[derive(Clone)]
    struct KeepFirst;
    impl Agent for KeepFirst {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { reason: SelectReason::Generic, from, .. } if !from.is_empty() => {
                    DecisionResponse::Indices(vec![0])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Resolve the trigger effect directly with the triggering spell carrying `cast_x = Some(3)`:
    /// look at the top 3, keep one (hand +1), the other two go to the bottom of the library.
    #[test]
    fn looks_at_x_cards_and_keeps_one() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        // Five cards in P0's library so "top 3" is well-defined.
        for _ in 0..5 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library);
        }
        // A stand-in "triggering spell" object carrying the chosen X = 3.
        let spell = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Stack)
        };
        state.objects.get_mut(&spell).unwrap().cast_x = Some(3);
        let effect = state.card_db().get(GEOMETERS_ARTHROPOD).unwrap().abilities[0].clone();
        let effect = match effect {
            Ability::Triggered { effect, .. } => effect,
            _ => unreachable!(),
        };
        let mut e = Engine::new(state, vec![Box::new(KeepFirst), Box::new(KeepFirst)]);
        let hand_before = e.state.player(PlayerId(0)).hand.len();
        let lib_before = e.state.player(PlayerId(0)).library.len();
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                triggering_spell: Some(spell),
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand_before + 1, "kept one card");
        assert_eq!(
            e.state.player(PlayerId(0)).library.len(),
            lib_before - 1,
            "the other two returned to the bottom; net library -1"
        );
    }

    /// A triggering spell with no {X} (`cast_x = None`) yields X = 0 → look at nothing, keep nothing.
    #[test]
    fn no_x_means_no_cards() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        for _ in 0..3 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library);
        }
        let spell = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Stack)
        };
        // cast_x left None.
        let effect = match state.card_db().get(GEOMETERS_ARTHROPOD).unwrap().abilities[0].clone() {
            Ability::Triggered { effect, .. } => effect,
            _ => unreachable!(),
        };
        let mut e = Engine::new(state, vec![Box::new(KeepFirst), Box::new(KeepFirst)]);
        let hand_before = e.state.player(PlayerId(0)).hand.len();
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                triggering_spell: Some(spell),
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand_before, "X=0 → no card kept");
    }
}
