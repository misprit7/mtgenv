//! Seize the Spoils — `{2}{R}` Sorcery (first printed SOS).
//!
//! Oracle: "As an additional cost to cast this spell, discard a card. Draw two cards and create a
//! Treasure token."
//!
//! **Fully implemented** — the lander for the **spell-level additional cast cost** cap (CR
//! 601.2b/f). The additional "discard a card" rides as an [`Ability::AdditionalCost`] marker (a
//! single-option clause): the offer gate requires a discardable card *other than* this spell
//! (which is already on the stack), and `cast_spell` pays the discard through the real cost
//! machinery at 601.2f–h — so the card is discarded **at cast**, not on resolution (a countered
//! Seize the Spoils has still paid it). The spell effect is a plain `Draw 2` + one Treasure token
//! (the shared [`helpers::treasure_token`]).

use crate::basics::{CardType, Color, Zone};
use crate::cards::{helpers, mana_cost, spell, CardDb};
use crate::effects::ability::{AdditionalCost, Ability, Cost, CostComponent};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const SEIZE_THE_SPOILS: u32 = 411;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Draw {
            who: PlayerRef::Controller,
            count: ValueExpr::Fixed(2),
        },
        Effect::CreateToken {
            spec: helpers::treasure_token(),
            count: ValueExpr::Fixed(1),
            controller: PlayerRef::Controller,
            dynamic_counters: vec![],
        },
    ]);
    let mut def = spell(
        SEIZE_THE_SPOILS,
        "Seize the Spoils",
        CardType::Sorcery,
        Color::Red,
        mana_cost(2, &[(Color::Red, 1)]),
        effect,
    )
    .with_text(
        "As an additional cost to cast this spell, discard a card.\nDraw two cards and create a Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")",
    );
    // "As an additional cost to cast this spell, discard a card." (CR 601.2b) — a single-option
    // clause; paid at cast, gated by the offer (a discardable card must exist besides this spell).
    def.abilities.push(Ability::AdditionalCost(AdditionalCost {
        options: vec![Cost {
            mana: None,
            components: vec![CostComponent::Discard(SelectSpec {
                zone: Zone::Hand,
                filter: CardFilter::Any,
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(1),
                max: ValueExpr::Fixed(1),
            })],
        }],
    }));
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayableAction, PlayerView, RandomAgent};
    use crate::basics::Phase;
    use crate::cards::{build_game, grp};
    use crate::ids::{ObjId, PlayerId};
    use crate::priority::Engine;
    use expect_test::expect;

    #[test]
    fn seize_the_spoils_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SEIZE_THE_SPOILS).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert!(def.fully_implemented);
        // One additional-cost clause: discard a card.
        let ac = def.additional_costs();
        assert_eq!(ac.len(), 1, "one additional-cost clause");
        assert_eq!(ac[0].options.len(), 1, "single-option (non-modal)");
        assert!(matches!(ac[0].options[0].components[0], CostComponent::Discard(_)));
        expect![[r#"
            Sequence(
                [
                    Draw {
                        who: Controller,
                        count: Fixed(
                            2,
                        ),
                    },
                    CreateToken {
                        spec: TokenSpec {
                            name: "Treasure",
                            card_types: [
                                Artifact,
                            ],
                            subtypes: [
                                Artifact(
                                    Treasure,
                                ),
                            ],
                            colors: [],
                            power: 0,
                            toughness: 0,
                            keywords: [],
                            counters: [],
                            grp_id: 9002,
                        },
                        count: Fixed(
                            1,
                        ),
                        controller: Controller,
                        dynamic_counters: [],
                    },
                ],
            )"#]]
        .assert_eq(&format!("{:#?}", def.spell_effect().unwrap()));
    }

    /// Put Seize the Spoils + `hand_extra` other cards in P0's hand, `lands` Mountains untapped, and
    /// `lib` cards in P0's library. Returns `(engine, seize)`.
    fn setup(hand_extra: usize, lands: usize, lib: usize, agent: Box<dyn Agent>) -> (Engine, ObjId) {
        let mut state = build_game(1, &[&[], &[]]);
        let seize = state.add_card(
            PlayerId(0),
            state.card_db().get(SEIZE_THE_SPOILS).unwrap().chars.clone(),
            Zone::Hand,
        );
        for _ in 0..hand_extra {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand);
        }
        for _ in 0..lands {
            let c = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        for _ in 0..lib {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![agent, Box::new(RandomAgent::new(1))]);
        (e, seize)
    }

    /// Offer gate: castable only when there's a card to discard as the additional cost — with just
    /// Seize the Spoils in hand (nothing else to discard, and it can't discard itself), it's not
    /// offered even with the mana to pay {2}{R}.
    #[test]
    fn offered_only_with_a_card_to_discard() {
        let offered = |hand_extra: usize| {
            let (e, _) = setup(hand_extra, 3, 2, Box::new(RandomAgent::new(0)));
            e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::Cast { .. }))
        };
        assert!(!offered(0), "only Seize in hand → nothing to discard → not offered");
        assert!(offered(1), "a second card in hand → discardable → offered");
    }

    /// Offer gate also still requires the mana: with a card to discard but no lands, not offered.
    #[test]
    fn not_offered_without_mana() {
        let (e, _) = setup(1, 0, 2, Box::new(RandomAgent::new(0)));
        assert!(
            !e.legal_actions(PlayerId(0)).iter().any(|a| matches!(a, PlayableAction::Cast { .. })),
            "no mana → not offered despite a discardable card"
        );
    }

    /// An agent that discards the first offered card when asked, else passes.
    #[derive(Clone)]
    struct DiscardFirst;
    impl Agent for DiscardFirst {
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

    /// Real cast path: the discard is paid **at cast** (a hand card moves to the graveyard before
    /// resolution), and resolving draws 2 and creates a Treasure token.
    #[test]
    fn discards_at_cast_then_draws_two_and_makes_a_treasure() {
        let (mut e, seize) = setup(1, 3, 2, Box::new(DiscardFirst));
        let hand_before = e.state.player(PlayerId(0)).hand.len();
        assert_eq!(hand_before, 2, "Seize + one discard fodder");

        e.cast_spell(PlayerId(0), seize, CastVariant::Normal);
        // The additional discard happened at cast: the fodder card is in the graveyard, and the
        // spell (Seize) is on the stack — neither is in hand.
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 0, "fodder discarded, Seize on stack");
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 1, "the discarded card");
        assert!(!e.state.player(PlayerId(0)).graveyard.contains(&seize), "Seize is on the stack, not discarded");
        // All three Mountains tapped for {2}{R}.
        let tapped = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .filter(|&&id| e.state.object(id).chars.is_land() && e.state.object(id).status.tapped)
            .count();
        assert_eq!(tapped, 3, "paid {{2}}{{R}} = 3 mana");

        e.resolve_top();
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 2, "drew two cards");
        assert!(
            e.state
                .player(PlayerId(0))
                .battlefield
                .iter()
                .any(|&id| e.state.object(id).chars.grp_id == grp::TREASURE_TOKEN),
            "a Treasure token was created"
        );
    }
}
