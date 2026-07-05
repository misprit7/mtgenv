//! Harmonized Trio // Brainstorm — `{U}` Creature — Merfolk Bard Wizard 1/1 // `{U}` Instant (first
//! printed SOS). A **Prepare** DFC — the "activated ability with a tap-others cost" variant.
//!
//! Front oracle: "{T}, Tap two untapped creatures you control: This creature becomes prepared. (While
//! it's prepared, you may cast a copy of its spell. Doing so unprepares it.)"
//! Back oracle (Brainstorm): "Draw three cards, then put two cards from your hand on top of your library
//! in any order."
//!
//! **Fully implemented.** The front prepares via an activated ability whose cost is `{T}` plus the new
//! [`CostComponent::TapCreatures`] (tap two *other* untapped creatures you control — a count-based
//! sibling of Crew, reusing the crew payment path). The back is `Sequence[Draw 3, PutFromHandOnTop 2]`
//! via the new [`Effect::PutFromHandOnTop`] (select two hand cards, ordered, onto the library top).

use crate::basics::{CardType, Color};
use crate::cards::{creature, mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const HARMONIZED_TRIO: u32 = 407;
/// The copy-only back-face spell (reserved 9700+ Prepare block).
pub const BRAINSTORM: u32 = 9732;

pub fn register(db: &mut CardDb) {
    // Back face — "Brainstorm" ({U} Instant): draw 3, then put 2 from hand on top of library in any order.
    db.insert(
        spell(
            BRAINSTORM,
            "Brainstorm",
            CardType::Instant,
            Color::Blue,
            mana_cost(0, &[(Color::Blue, 1)]),
            Effect::Sequence(vec![
                Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(3) },
                Effect::PutFromHandOnTop { who: PlayerRef::Controller, count: ValueExpr::Fixed(2) },
            ]),
        )
        .with_text("Draw three cards, then put two cards from your hand on top of your library in any order."),
    );

    // Front face — the 1/1; enters via a `{T}, Tap two untapped creatures you control:` activated
    // prepare ability (no timing restriction → instant speed).
    let mut front = creature(
        HARMONIZED_TRIO,
        "Harmonized Trio",
        &[CreatureType::Merfolk, CreatureType::Bard, CreatureType::Wizard],
        Color::Blue,
        mana_cost(0, &[(Color::Blue, 1)]),
        1,
        1,
        vec![
            Ability::Prepare { spell: BRAINSTORM },
            Ability::Activated {
                cost: Cost {
                    mana: None,
                    components: vec![CostComponent::TapSelf, CostComponent::TapCreatures(2)],
                },
                effect: Effect::BecomePrepared,
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            },
        ],
    );
    front.text = "{T}, Tap two untapped creatures you control: This creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Brainstorm {U} Instant — Draw three cards, then put two cards from your hand on top of your library in any order.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, AbilityRef, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::cards::grp;
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use crate::state::GameState;

    fn db_with_card() -> CardDb {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        db
    }

    fn add(state: &mut GameState, who: PlayerId, grp_id: u32, zone: Zone) -> ObjId {
        let c = state.card_db().get(grp_id).unwrap().chars.clone();
        state.add_card(who, c, zone)
    }

    #[test]
    fn harmonized_trio_ir() {
        let db = db_with_card();
        let front = db.get(HARMONIZED_TRIO).unwrap();
        assert!(matches!(front.abilities[0], Ability::Prepare { spell: BRAINSTORM }));
        match &front.abilities[1] {
            Ability::Activated { cost, .. } => {
                assert_eq!(
                    cost.components,
                    vec![CostComponent::TapSelf, CostComponent::TapCreatures(2)]
                );
            }
            _ => panic!("expected an activated prepare ability"),
        }
        let back = db.get(BRAINSTORM).unwrap();
        assert_eq!(back.chars.card_types, vec![CardType::Instant]);
        assert!(matches!(
            back.spell_effect(),
            Some(Effect::Sequence(_))
        ));
    }

    /// For SelectCards taps the first two offered creatures; passes otherwise.
    struct TapTwo;
    impl Agent for TapTwo {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { from, .. } if from.len() >= 2 => {
                    DecisionResponse::Indices(vec![0, 1])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Activate the front's ability (index 1): pay `{T}` (tap the Trio) + tap two other creatures → the
    /// Trio becomes prepared, and exactly two other creatures end up tapped.
    #[test]
    fn tap_two_creatures_prepares_the_trio() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        let trio = add(&mut state, PlayerId(0), HARMONIZED_TRIO, Zone::Battlefield);
        // Two other creatures to tap.
        let a = add(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let b = add(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        // The Trio must have been under control since your last turn to use {T} (clear summoning sick).
        state.objects.get_mut(&trio).unwrap().summoning_sick = false;
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(TapTwo), Box::new(TapTwo)]);

        e.activate_ability(PlayerId(0), trio, AbilityRef(1));
        e.resolve_top();

        assert!(e.state.object(trio).prepared, "the Trio became prepared");
        assert!(e.state.object(trio).status.tapped, "{{T}} tapped the Trio itself");
        let tapped_others = [a, b].iter().filter(|&&o| e.state.object(o).status.tapped).count();
        assert_eq!(tapped_others, 2, "two other creatures were tapped as the cost");
    }

    /// Brainstorm: draw three, then put two from hand on top of the library, first-chosen on top.
    #[test]
    fn brainstorm_draws_three_and_puts_two_back() {
        struct PickBack;
        impl Agent for PickBack {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { from, .. } if from.len() >= 2 => {
                        DecisionResponse::Indices(vec![0, 1])
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }
        let mut state = GameState::new(2, 1);
        state.set_card_db(std::sync::Arc::new(db_with_card()));
        // Library top-to-bottom: three named cards to draw.
        let top3 = [grp::ISLAND, grp::GRIZZLY_BEARS, grp::FOREST];
        for &g in top3.iter().rev() {
            add(&mut state, PlayerId(0), g, Zone::Library);
        }
        // Some cards already in hand so the put-back pool is nonempty & deterministic.
        add(&mut state, PlayerId(0), grp::PLAINS, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let hand_before = state.player(PlayerId(0)).hand.len();
        let lib_before = state.player(PlayerId(0)).library.len();
        let effect = state.card_db().get(BRAINSTORM).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(PickBack), Box::new(PickBack)]);
        e.resolve_effect(
            &effect,
            &crate::effects::action::ResolutionCtx {
                controller: Some(PlayerId(0)),
                ..Default::default()
            },
            crate::effects::action::WbReason::Resolve(crate::ids::StackId(99)),
        );
        // Net hand: +3 drawn, -2 put back = +1. Net library: -3 drawn, +2 back = -1.
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand_before + 1, "net +1 card in hand");
        assert_eq!(e.state.player(PlayerId(0)).library.len(), lib_before - 1, "net -1 card in library");
    }
}
