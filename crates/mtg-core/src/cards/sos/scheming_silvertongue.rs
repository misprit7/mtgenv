//! Scheming Silvertongue // Sign in Blood — `{1}{B}` Creature — Vampire Warlock 1/3 // `{B}{B}` Sorcery
//! (first printed SOS). A **Prepare** DFC — the "second main phase, if you gained life" variant.
//!
//! Front: "At the beginning of your second main phase, if you gained 2 or more life this turn, this
//! creature becomes prepared."
//! Back (Sign in Blood): "Target player draws two cards and loses 2 life."
//!
//! **Fully implemented** — the prepare trigger is a `BeginningOfStep(PostcombatMain)` ability gated by
//! `All(YourTurn, LifeGainedThisTurn ≥ 2)` whose effect is [`Effect::BecomePrepared`]. The back is a
//! target-player draw-two-lose-two (the same shape as Decorum Dissertation's underlying effect).

use crate::basics::{CardType, Color, Phase};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::target::PlayerFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

pub const SCHEMING_SILVERTONGUE: u32 = 394;
pub const SIGN_IN_BLOOD: u32 = 9721;

pub fn register(db: &mut CardDb) {
    let sign_in_blood = Effect::Sequence(vec![
        Effect::TargetPlayer(PlayerFilter::Any),
        Effect::Draw { who: PlayerRef::ChosenTarget(0), count: ValueExpr::Fixed(2) },
        Effect::LoseLife { who: PlayerRef::ChosenTarget(0), amount: ValueExpr::Fixed(2) },
    ]);
    db.insert(
        spell(SIGN_IN_BLOOD, "Sign in Blood", CardType::Sorcery, Color::Black, mana_cost(0, &[(Color::Black, 2)]), sign_in_blood)
            .with_text("Target player draws two cards and loses 2 life."),
    );
    let cond = Condition::All(vec![
        Condition::YourTurn,
        Condition::ValueAtLeast(
            ValueExpr::LifeGainedThisTurn { who: PlayerRef::Controller },
            ValueExpr::Fixed(2),
        ),
    ]);
    let mut front = creature(
        SCHEMING_SILVERTONGUE,
        "Scheming Silvertongue",
        &[CreatureType::Vampire, CreatureType::Warlock],
        Color::Black,
        mana_cost(1, &[(Color::Black, 1)]),
        1,
        3,
        helpers::prepared_abilities(SIGN_IN_BLOOD, EventPattern::BeginningOfStep(Phase::PostcombatMain), Some(cond), false),
    );
    front.text = "At the beginning of your second main phase, if you gained 2 or more life this turn, this creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Sign in Blood {B}{B} Sorcery — Target player draws two cards and loses 2 life.".to_string();
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

    struct Yes;
    impl Agent for Yes {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                DecisionRequest::ChooseTargets { slots, .. } => DecisionResponse::Pairs(
                    slots.iter().enumerate().map(|(si, _)| (si as u32, 0u32)).collect(),
                ),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// The prepare trigger fires at the second main phase only when 2+ life was gained this turn.
    #[test]
    fn second_main_prepares_only_after_gaining_two_life() {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let silvertongue = {
            let c = state.card_db().get(SCHEMING_SILVERTONGUE).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let _ = grp::SWAMP;
        let mut e = Engine::new(state, vec![Box::new(Yes), Box::new(Yes)]);
        e.state.active_player = PlayerId(0);
        e.state.phase = Phase::PostcombatMain;

        // No life gained yet → the trigger does not fire.
        e.broadcast(GameEvent::PhaseBegan { turn: 1, phase: Phase::PostcombatMain, active: PlayerId(0) });
        e.run_agenda();
        assert!(!e.state.object(silvertongue).prepared, "no life gained → not prepared");

        // Gain 2 life, then the second main phase prepares it.
        e.state.player_mut(PlayerId(0)).life_gained_this_turn = 2;
        e.broadcast(GameEvent::PhaseBegan { turn: 1, phase: Phase::PostcombatMain, active: PlayerId(0) });
        e.run_agenda();
        e.resolve_top();
        assert!(e.state.object(silvertongue).prepared, "gained 2+ life → prepared");
    }
}
