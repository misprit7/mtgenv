//! Lluwen, Exchange Student // Pest Friend — `{2}{B}{G}` Legendary Creature — Elf Druid 3/4 //
//! `{B/G}` Sorcery (first printed SOS). A **Prepare** DFC — the "activated ability" variant (it also
//! enters prepared).
//!
//! Front oracle: "Lluwen enters prepared. Exile a creature card from your graveyard: Lluwen becomes
//! prepared. Activate only as a sorcery."
//! Back oracle (Pest Friend): "Create a 1/1 black and green Pest creature token with 'Whenever this
//! token attacks, you gain 1 life.'"
//!
//! **Fully implemented** — two ways to become prepared, each an ordinary ability whose effect is
//! [`Effect::BecomePrepared`]: a `SelfEnters` trigger (enters prepared) and a sorcery-speed activated
//! ability whose cost is exiling a creature card from your graveyard ([`CostComponent::Exile`]). The
//! back face creates the shared [`helpers::pest_token`]. Exercises the prepared-cast rails from an
//! **activated** source (vs the triggered sources of the other representative cards).

use crate::basics::{CardType, Color, Zone};
use crate::cards::helpers;
use crate::cards::{creature, mana_cost, mana_cost_hybrid, spell, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const LLUWEN_EXCHANGE_STUDENT: u32 = 376;
/// The copy-only back-face spell (reserved 9700+ Prepare block).
pub const PEST_FRIEND: u32 = 9703;

pub fn register(db: &mut CardDb) {
    // Back face — "Pest Friend" ({B/G} Sorcery): create one Pest token.
    let mut back = spell(
        PEST_FRIEND,
        "Pest Friend",
        CardType::Sorcery,
        Color::Black,
        mana_cost_hybrid(0, &[], &[(Color::Black, Color::Green)]),
        Effect::CreateToken {
            spec: helpers::pest_token(),
            count: ValueExpr::Fixed(1),
            controller: PlayerRef::Controller,
            dynamic_counters: Vec::new(),
        },
    )
    .with_text(
        "Create a 1/1 black and green Pest creature token with \"Whenever this token attacks, you gain 1 life.\"",
    );
    back.chars.colors = vec![Color::Black, Color::Green];
    db.insert(back);

    // Front face — legendary; enters prepared, and a sorcery-speed activated ability (exile a creature
    // card from your graveyard) also prepares it.
    let mut front = creature(
        LLUWEN_EXCHANGE_STUDENT,
        "Lluwen, Exchange Student",
        &[CreatureType::Elf, CreatureType::Druid],
        Color::Black,
        mana_cost(2, &[(Color::Black, 1), (Color::Green, 1)]),
        3,
        4,
        vec![
            Ability::Prepare { spell: PEST_FRIEND },
            Ability::Triggered {
                event: crate::effects::ability::EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::BecomePrepared,
            },
            // "Exile a creature card from your graveyard: Lluwen becomes prepared. Activate only as a
            // sorcery." A pure non-mana cost — exile another creature card from your graveyard.
            Ability::Activated {
                cost: Cost {
                    mana: None,
                    components: vec![CostComponent::Exile(SelectSpec {
                        zone: Zone::Graveyard,
                        filter: CardFilter::HasCardType(CardType::Creature),
                        chooser: PlayerRef::Controller,
                        min: ValueExpr::Fixed(1),
                        max: ValueExpr::Fixed(1),
                    })],
                },
                effect: Effect::BecomePrepared,
                timing: Timing::Sorcery,
                restriction: None,
                is_mana: false,
            },
        ],
    );
    front.chars.colors = vec![Color::Black, Color::Green];
    front.chars.supertypes = vec![Supertype::Legendary];
    front.text = "Lluwen enters prepared. Exile a creature card from your graveyard: Lluwen becomes prepared. Activate only as a sorcery. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Pest Friend {B/G} Sorcery — Create a 1/1 black and green Pest creature token with \"Whenever this token attacks, you gain 1 life.\"".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, AbilityRef, DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
    use crate::basics::Phase;
    use crate::cards::grp;
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use expect_test::expect;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn lluwen_ir() {
        let db = db_with_card();
        let front = db.get(LLUWEN_EXCHANGE_STUDENT).unwrap();
        assert_eq!(front.chars.supertypes, vec![Supertype::Legendary]);
        expect![[r#"
            [
                Prepare {
                    spell: 9703,
                },
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: BecomePrepared,
                },
                Activated {
                    cost: Cost {
                        mana: None,
                        components: [
                            Exile(
                                SelectSpec {
                                    zone: Graveyard,
                                    filter: HasCardType(
                                        Creature,
                                    ),
                                    chooser: Controller,
                                    min: Fixed(
                                        1,
                                    ),
                                    max: Fixed(
                                        1,
                                    ),
                                },
                            ),
                        ],
                    },
                    effect: BecomePrepared,
                    timing: Sorcery,
                    restriction: None,
                    is_mana: false,
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", front.abilities));
    }

    /// Yes to confirms; picks candidate 0 for target/select choices.
    struct PrepareAgent;
    impl Agent for PrepareAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                DecisionRequest::ChooseTargets { slots, .. } => DecisionResponse::Pairs(
                    slots.iter().enumerate().map(|(si, _)| (si as u32, 0u32)).collect(),
                ),
                DecisionRequest::SelectCards { min, max, from, .. } => {
                    let n = (*min).max(1).min(*max).min(from.len() as u32);
                    DecisionResponse::Indices((0..n).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Activate the exile-a-creature-from-graveyard prepare ability → prepared → cast the copy of Pest
    /// Friend (which creates a Pest token) → unprepared. Drives the prepared-cast rails from an
    /// **activated** source.
    #[test]
    fn activated_ability_prepares_then_casts_pest_friend_copy() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let lluwen = {
            let c = state.card_db().get(LLUWEN_EXCHANGE_STUDENT).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, crate::basics::Zone::Battlefield)
        };
        // A creature card in the graveyard funds the exile cost; a Swamp pays the {B/G} copy.
        let fodder = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, crate::basics::Zone::Graveyard)
        };
        let s = state.card_db().get(grp::SWAMP).unwrap().chars.clone();
        state.add_card(PlayerId(0), s, crate::basics::Zone::Battlefield);
        let mut e = Engine::new(state, vec![Box::new(PrepareAgent), Box::new(PrepareAgent)]);
        e.state.active_player = PlayerId(0);
        e.state.phase = Phase::PrecombatMain;

        // Activate the exile-cost prepare ability (index 2), which pays by exiling the fodder creature.
        e.activate_ability(PlayerId(0), lluwen, AbilityRef(2));
        e.resolve_top();
        assert!(e.state.object(lluwen).prepared, "the activated ability prepared Lluwen");
        assert!(e.state.player(PlayerId(0)).exile.contains(&fodder), "exile cost paid (fodder exiled)");
        assert!(
            e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::CastPrepared { source } if *source == lluwen)),
        );

        // Cast the copy of Pest Friend: a Pest token enters, and Lluwen is unprepared.
        e.cast_prepared(PlayerId(0), lluwen);
        e.resolve_top();
        let pest_made = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .any(|&o| e.state.object(o).chars.grp_id == grp::PEST_TOKEN);
        assert!(pest_made, "the Pest Friend copy created a Pest token");
        assert!(!e.state.object(lluwen).prepared, "casting the copy unprepared Lluwen");
    }
}
