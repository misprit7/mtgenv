//! Emeritus of Woe // Demonic Tutor — `{3}{B}` Creature — Vampire Warlock 5/4 // `{1}{B}` Sorcery
//! (first printed SOS). A **Prepare** DFC — enters prepared, and re-prepares at your end step after a
//! bloodbath.
//!
//! Front: "This creature enters prepared. At the beginning of your end step, if two or more creatures
//! died this turn, this creature becomes prepared."
//! Back (Demonic Tutor): "Search your library for a card, put that card into your hand, then shuffle."
//!
//! **Fully implemented** — enters-prepared plus a `BeginningOfStep(End)` trigger gated by
//! `All(YourTurn, CreaturesDiedThisTurn ≥ 2)`; both effects are [`Effect::BecomePrepared`]. The back
//! is an unrestricted tutor to hand.

use crate::basics::{CardType, Color, Phase, Zone, ZoneDest, ZonePos};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

pub const EMERITUS_OF_WOE: u32 = 395;
pub const DEMONIC_TUTOR: u32 = 9722;

pub fn register(db: &mut CardDb) {
    let demonic_tutor = Effect::Search {
        who: PlayerRef::Controller,
        zone: Zone::Library,
        filter: CardFilter::Any,
        min: 0,
        max: 1,
        to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
        tapped: false,
    };
    db.insert(
        spell(DEMONIC_TUTOR, "Demonic Tutor", CardType::Sorcery, Color::Black, mana_cost(1, &[(Color::Black, 1)]), demonic_tutor)
            .with_text("Search your library for a card, put that card into your hand, then shuffle."),
    );

    let mut abilities = helpers::enters_prepared(DEMONIC_TUTOR);
    abilities.push(Ability::Triggered {
        event: EventPattern::BeginningOfStep(Phase::End),
        condition: Some(Condition::All(vec![
            Condition::YourTurn,
            Condition::ValueAtLeast(ValueExpr::CreaturesDiedThisTurn, ValueExpr::Fixed(2)),
        ])),
        intervening_if: false,
        effect: Effect::BecomePrepared,
    });
    let mut front = creature(
        EMERITUS_OF_WOE,
        "Emeritus of Woe",
        &[CreatureType::Vampire, CreatureType::Warlock],
        Color::Black,
        mana_cost(3, &[(Color::Black, 1)]),
        5,
        4,
        abilities,
    );
    front.text = "This creature enters prepared. At the beginning of your end step, if two or more creatures died this turn, this creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Demonic Tutor {1}{B} Sorcery — Search your library for a card, put that card into your hand, then shuffle.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    struct Yes;
    impl Agent for Yes {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                DecisionRequest::SelectCards { from, min, max, .. } => {
                    let n = (*min).max(1).min(*max).min(from.len() as u32);
                    DecisionResponse::Indices((0..n).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn end_step_prepares_after_two_creatures_die() {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let woe = {
            let c = state.card_db().get(EMERITUS_OF_WOE).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let mut e = Engine::new(state, vec![Box::new(Yes), Box::new(Yes)]);
        e.state.active_player = PlayerId(0);
        e.state.phase = Phase::End;
        // Casting-independent: mark that two creatures died this turn under P0's control.
        e.state.player_mut(PlayerId(0)).creatures_died_this_turn = 2;
        e.broadcast(GameEvent::PhaseBegan { turn: 1, phase: Phase::End, active: PlayerId(0) });
        e.run_agenda();
        e.resolve_top();
        assert!(e.state.object(woe).prepared, "two deaths at your end step → prepared");
    }
}
