//! Soaring Stoneglider — `{2}{W}` Creature — Elephant Cleric 4/3 (first printed SOS).
//!
//! Oracle: "As an additional cost to cast this spell, exile two cards from your graveyard or pay
//! {1}{W}. Flying, vigilance."
//!
//! **Fully implemented** — the lander for a **modal** additional cast cost (CR 601.2b): the single
//! [`Ability::AdditionalCost`] clause carries two options — exile two cards from your graveyard, OR
//! pay `{1}{W}` — and the caster pays whichever is payable (both offered when both can be paid). The
//! offer gate requires at least one option payable; `cast_spell` folds a chosen mana option into the
//! mana payment and pays the exile through the real cost machinery. A vanilla 4/3 with Flying +
//! Vigilance otherwise.

use crate::basics::{Color, Zone};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{AdditionalCost, Ability, Cost, CostComponent, Keyword};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const SOARING_STONEGLIDER: u32 = 414;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        SOARING_STONEGLIDER,
        "Soaring Stoneglider",
        &[CreatureType::Elephant, CreatureType::Cleric],
        Color::White,
        mana_cost(2, &[(Color::White, 1)]),
        4,
        3,
        vec![Ability::AdditionalCost(AdditionalCost {
            options: vec![
                // Exile two cards from your graveyard, …
                Cost {
                    mana: None,
                    components: vec![CostComponent::Exile(SelectSpec {
                        zone: Zone::Graveyard,
                        filter: CardFilter::Any,
                        chooser: PlayerRef::Controller,
                        min: ValueExpr::Fixed(2),
                        max: ValueExpr::Fixed(2),
                    })],
                },
                // … or pay {1}{W}.
                Cost { mana: Some(mana_cost(1, &[(Color::White, 1)])), components: vec![] },
            ],
        })],
    );
    def.chars.keywords = vec![Keyword::Flying, Keyword::Vigilance];
    def.text =
        "As an additional cost to cast this spell, exile two cards from your graveyard or pay {1}{W}.\nFlying, vigilance"
            .to_string();
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

    #[test]
    fn soaring_stoneglider_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SOARING_STONEGLIDER).unwrap();
        assert_eq!(def.chars.power, Some(4));
        assert_eq!(def.chars.toughness, Some(3));
        assert_eq!(def.chars.keywords, vec![Keyword::Flying, Keyword::Vigilance]);
        assert!(def.fully_implemented);
        let ac = def.additional_costs();
        assert_eq!(ac.len(), 1, "one clause");
        assert_eq!(ac[0].options.len(), 2, "modal: exile-two OR pay {{1}}{{W}}");
        assert!(matches!(ac[0].options[0].components[0], CostComponent::Exile(_)));
        assert!(ac[0].options[1].mana.is_some(), "the {{1}}{{W}} option");
    }

    /// Put Soaring Stoneglider in P0's hand with `plains` Plains untapped and `gy` filler cards in
    /// P0's graveyard. Returns `(engine, glider)`.
    fn setup(plains: usize, gy: usize, agent: Box<dyn Agent>) -> (Engine, ObjId) {
        let mut state = build_game(1, &[&[], &[]]);
        let glider = state.add_card(
            PlayerId(0),
            state.card_db().get(SOARING_STONEGLIDER).unwrap().chars.clone(),
            Zone::Hand,
        );
        for _ in 0..plains {
            let c = state.card_db().get(grp::PLAINS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        for _ in 0..gy {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Graveyard);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![agent, Box::new(RandomAgent::new(1))]);
        (e, glider)
    }

    /// Offer gate: castable if EITHER additional-cost option is payable — via the exile option
    /// (2+ graveyard cards, base {2}{W}) or via the {1}{W} option (extra mana), but not when neither
    /// is available (base {2}{W} only, empty graveyard).
    #[test]
    fn offered_when_either_option_is_payable() {
        let offered = |plains: usize, gy: usize| {
            let (e, _) = setup(plains, gy, Box::new(RandomAgent::new(0)));
            e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::Cast { .. }))
        };
        // 3 Plains = exactly {2}{W}; empty graveyard → neither option payable → not offered.
        assert!(!offered(3, 0), "base mana only, no graveyard → neither option → not offered");
        // 3 Plains + 2 graveyard cards → exile option payable → offered.
        assert!(offered(3, 2), "2 graveyard cards → exile option → offered");
        // 5 Plains ({2}{W} + {1}{W}) + empty graveyard → pay option payable → offered.
        assert!(offered(5, 0), "{{1}}{{W}} extra mana → pay option → offered");
    }

    /// An agent that answers `SelectCards` by taking the first `min` offered, else passes. Used for
    /// the exile-two-cards option (no modal `ChooseNumber` when only that option is payable).
    #[derive(Clone)]
    struct ExileAgent;
    impl Agent for ExileAgent {
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

    /// Real cast via the exile option (only one payable → no modal choice): two graveyard cards are
    /// exiled at cast and the 4/3 enters. With 3 graveyard cards the payer chooses which two.
    #[test]
    fn casts_by_exiling_two_graveyard_cards() {
        let (mut e, glider) = setup(3, 3, Box::new(ExileAgent));
        e.cast_spell(PlayerId(0), glider, CastVariant::Normal);
        assert_eq!(e.state.player(PlayerId(0)).exile.len(), 2, "two cards exiled as the cost");
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 1, "one graveyard card left");
        e.resolve_top();
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&glider), "the 4/3 resolved onto the battlefield");
    }

    /// Real cast via the {1}{W} option (only one payable, empty graveyard): {2}{W} + {1}{W} = five
    /// mana paid, nothing exiled, the 4/3 enters.
    #[test]
    fn casts_by_paying_one_white() {
        let (mut e, glider) = setup(5, 0, Box::new(RandomAgent::new(0)));
        e.cast_spell(PlayerId(0), glider, CastVariant::Normal);
        let tapped = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .filter(|&&id| e.state.object(id).chars.is_land() && e.state.object(id).status.tapped)
            .count();
        assert_eq!(tapped, 5, "paid {{2}}{{W}} + {{1}}{{W}} = 5 mana");
        assert!(e.state.player(PlayerId(0)).exile.is_empty(), "nothing exiled");
        e.resolve_top();
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&glider));
    }

    /// An agent that picks additional-cost option `pick` at the modal `ChooseNumber`, and takes the
    /// first `min` at `SelectCards`.
    #[derive(Clone)]
    struct ModalAgent(i64);
    impl Agent for ModalAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseNumber { .. } => DecisionResponse::Number(self.0),
                DecisionRequest::SelectCards { min, max, from, .. } => {
                    let n = (*min).max(1).min(*max).min(from.len() as u32);
                    DecisionResponse::Indices((0..n).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// When BOTH options are payable, the caster chooses. Picking option 1 (pay {1}{W}) leaves the
    /// graveyard intact; picking option 0 (exile two) leaves the graveyard shorter — same board.
    #[test]
    fn modal_choice_when_both_payable() {
        // Option 1: pay {1}{W} — graveyard untouched.
        let (mut e, glider) = setup(5, 2, Box::new(ModalAgent(1)));
        e.cast_spell(PlayerId(0), glider, CastVariant::Normal);
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 2, "chose to pay mana → graveyard intact");
        assert!(e.state.player(PlayerId(0)).exile.is_empty(), "nothing exiled");

        // Option 0: exile two from the graveyard — mana beyond {2}{W} untouched.
        let (mut e2, glider2) = setup(5, 2, Box::new(ModalAgent(0)));
        e2.cast_spell(PlayerId(0), glider2, CastVariant::Normal);
        assert_eq!(e2.state.player(PlayerId(0)).exile.len(), 2, "chose to exile two");
        assert!(e2.state.player(PlayerId(0)).graveyard.is_empty(), "both graveyard cards exiled");
    }
}
