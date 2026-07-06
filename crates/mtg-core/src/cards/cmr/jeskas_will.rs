//! Jeska's Will — `{2}{R}` Sorcery (first printed CMR; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Choose one. If you control a commander as you cast this spell, you may choose both instead.
//! • Add {R} for each card in target opponent's hand.
//! • Exile the top three cards of your library. You may play them this turn."
//!
//! **Fully implemented** for the limited environment (no commander → always "choose one"):
//! - Mode 1: a `TargetPlayer(Opponent)` + `AddMana` of `HandSize{ChosenTarget(0)}` red.
//! - Mode 2: three `ExileForPlay{ TopOfLibrary, ThisTurn }` (each exiles the current top and lets you
//!   play it this turn) — the impulse-play cap. The "choose both if you control a commander" clause is
//!   inapplicable (a 40-card limited deck has no commander), so it's a faithful choose-one here.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{ManaSpec, PlayerFilter};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, Mode, PlayWindow};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const JESKAS_WILL: u32 = 638;

pub fn register(db: &mut CardDb) {
    let ritual = Effect::Sequence(vec![
        Effect::TargetPlayer(PlayerFilter::Opponent),
        Effect::AddMana {
            who: PlayerRef::Controller,
            mana: ManaSpec {
                produces: vec![(Color::Red, ValueExpr::HandSize { who: PlayerRef::ChosenTarget(0) })],
                any_color: None,
                restriction: None,
            },
        },
    ]);
    let impulse = Effect::ExileTopForPlay {
        who: PlayerRef::Controller,
        count: ValueExpr::Fixed(3),
        window: PlayWindow::ThisTurn,
    };
    let effect = Effect::Modal {
        modes: vec![
            Mode { label: "Add {R} for each card in target opponent's hand".to_string(), effect: ritual },
            Mode { label: "Exile the top three cards of your library. You may play them this turn".to_string(), effect: impulse },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    db.insert(
        spell(
            JESKAS_WILL,
            "Jeska's Will",
            CardType::Sorcery,
            Color::Red,
            mana_cost(2, &[(Color::Red, 1)]),
            effect,
        )
        .with_text("Choose one. If you control a commander as you cast this spell, you may choose both instead.\n• Add {R} for each card in target opponent's hand.\n• Exile the top three cards of your library. You may play them this turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Target, Zone};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[derive(Clone)]
    struct Passive;
    impl Agent for Passive {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    /// Mode 1: opponent holds 3 cards → add 3 red mana.
    #[test]
    fn mode1_adds_red_per_opponent_card() {
        let mut state = build_game(1, &[&[], &[]]);
        for _ in 0..3 {
            state.add_card(PlayerId(1), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Hand);
        }
        let effect = state.card_db().get(JESKAS_WILL).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_modes: vec![0], chosen_targets: vec![Target::Player(PlayerId(1))], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).mana_pool.amounts.get(&Color::Red).copied().unwrap_or(0), 3, "3 red for 3 cards in opp hand");
    }

    /// Mode 2: exile the top three cards of the library (with play permission this turn).
    #[test]
    fn mode2_exiles_top_three() {
        let mut state = build_game(1, &[&[], &[]]);
        for _ in 0..5 {
            state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        }
        let effect = state.card_db().get(JESKAS_WILL).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_modes: vec![1], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).library.len(), 2, "three exiled from the top → two left");
        assert_eq!(e.state.player(PlayerId(0)).exile.len(), 3, "three cards exiled for play");
    }
}
