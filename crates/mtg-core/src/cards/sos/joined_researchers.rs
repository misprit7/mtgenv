//! Joined Researchers // Secret Rendezvous — `{1}{W}` Creature — Human Cleric Wizard 2/2 //
//! `{1}{W}{W}` Sorcery (first printed SOS). A **Prepare** DFC — the "each end step, if behind on
//! cards" variant.
//!
//! Front: "At the beginning of each end step, if an opponent has more cards in hand than you, this
//! creature becomes prepared."
//! Back (Secret Rendezvous): "You and target opponent each draw three cards."
//!
//! **Fully implemented** — the prepare trigger is a `BeginningOfStep(End)` ability (each end step, so
//! no YourTurn gate) with an intervening-if `HandSize(Opponent) ≥ HandSize(Controller) + 1` whose
//! effect is [`Effect::BecomePrepared`]. The back has you and a target opponent each draw three.

use crate::basics::{CardType, Color, Phase};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::EventPattern;
use crate::effects::condition::Condition;
use crate::effects::target::PlayerFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

pub const JOINED_RESEARCHERS: u32 = 398;
pub const SECRET_RENDEZVOUS: u32 = 9725;

pub fn register(db: &mut CardDb) {
    let secret_rendezvous = Effect::Sequence(vec![
        Effect::TargetPlayer(PlayerFilter::Opponent),
        Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(3) },
        Effect::Draw { who: PlayerRef::ChosenTarget(0), count: ValueExpr::Fixed(3) },
    ]);
    db.insert(
        spell(SECRET_RENDEZVOUS, "Secret Rendezvous", CardType::Sorcery, Color::White, mana_cost(1, &[(Color::White, 2)]), secret_rendezvous)
            .with_text("You and target opponent each draw three cards."),
    );
    // "an opponent has more cards in hand than you" — opp hand ≥ your hand + 1.
    let behind = Condition::ValueAtLeast(
        ValueExpr::HandSize { who: PlayerRef::Opponent },
        ValueExpr::Sum(
            Box::new(ValueExpr::HandSize { who: PlayerRef::Controller }),
            Box::new(ValueExpr::Fixed(1)),
        ),
    );
    let mut front = creature(
        JOINED_RESEARCHERS,
        "Joined Researchers",
        &[CreatureType::Human, CreatureType::Cleric, CreatureType::Wizard],
        Color::White,
        mana_cost(1, &[(Color::White, 1)]),
        2,
        2,
        helpers::prepared_abilities(SECRET_RENDEZVOUS, EventPattern::BeginningOfStep(Phase::End), Some(behind), true),
    );
    front.text = "At the beginning of each end step, if an opponent has more cards in hand than you, this creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Secret Rendezvous {1}{W}{W} Sorcery — You and target opponent each draw three cards.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
    use crate::basics::Zone;
    use crate::cards::grp;
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    struct Pass;
    impl Agent for Pass {
        fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    #[test]
    fn end_step_prepares_only_when_an_opponent_has_more_cards() {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let jr = {
            let c = state.card_db().get(JOINED_RESEARCHERS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        // P1 (opponent) holds two cards; P0 holds none → P0 is behind.
        for _ in 0..2 {
            let c = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Hand);
        }
        let mut e = Engine::new(state, vec![Box::new(Pass), Box::new(Pass)]);
        e.state.active_player = PlayerId(0);
        e.state.phase = Phase::End;
        e.broadcast(GameEvent::PhaseBegan { turn: 1, phase: Phase::End, active: PlayerId(0) });
        e.run_agenda();
        e.resolve_top();
        assert!(e.state.object(jr).prepared, "opponent has more cards → prepared");
    }
}
