//! Essenceknit Scholar — `{B}{B/G}{G}` Creature — Dryad Warlock 3/1 (first printed SOS).
//!
//! Oracle: "When this creature enters, create a 1/1 black and green Pest creature token with
//! \"Whenever this token attacks, you gain 1 life.\" / At the beginning of your end step, if a
//! creature died under your control this turn, draw a card."
//!
//! **Fully implemented** — hybrid cost (`{B/G}`) + an ETB Pest token (S11) + a "your end step, if a
//! creature died under your control this turn, draw" trigger (`CreatureDiedThisTurn` gate).

use crate::basics::{Color, Phase};
use crate::cards::helpers::pest_token;
use crate::cards::{creature, mana_cost_hybrid, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Condition;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ESSENCEKNIT_SCHOLAR: u32 = 296;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        ESSENCEKNIT_SCHOLAR,
        "Essenceknit Scholar",
        &[CreatureType::Dryad, CreatureType::Warlock],
        Color::Black,
        mana_cost_hybrid(0, &[(Color::Black, 1), (Color::Green, 1)], &[(Color::Black, Color::Green)]),
        3,
        1,
        vec![
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::CreateToken { spec: pest_token(), count: ValueExpr::Fixed(1), controller: PlayerRef::Controller, dynamic_counters: Vec::new() },
            },
            Ability::Triggered {
                event: EventPattern::BeginningOfStep(Phase::End),
                condition: Some(Condition::All(vec![
                    Condition::YourTurn,
                    Condition::CreatureDiedThisTurn { who: PlayerRef::Controller },
                ])),
                intervening_if: true,
                effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
            },
        ],
    );
    def.chars.colors = vec![Color::Black, Color::Green];
    def.text = "When this creature enters, create a 1/1 black and green Pest creature token with \"Whenever this token attacks, you gain 1 life.\"\nAt the beginning of your end step, if a creature died under your control this turn, draw a card.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn essenceknit_scholar_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ESSENCEKNIT_SCHOLAR).unwrap();
        assert_eq!(def.chars.colors, vec![Color::Black, Color::Green]);
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().hybrid, vec![(Color::Black, Color::Green)]);
        assert_eq!(def.chars.mana_value(), 3, "{{B}}{{B/G}}{{G}} = MV 3");
        assert!(def.fully_implemented);
    }

    /// The ETB makes an ability-bearing Pest; and a creature dying under your control sets the flag
    /// that gates the end-step draw.
    #[test]
    fn essenceknit_etb_pest_and_death_flag() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::{build_game, grp};
        use crate::conditions::holds_for_source;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let src = state.add_card(PlayerId(0), state.card_db().get(ESSENCEKNIT_SCHOLAR).unwrap().chars.clone(), Zone::Battlefield);
        let etb = match &state.card_db().get(ESSENCEKNIT_SCHOLAR).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(&etb, &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(src), ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert!(e.state.players[0].battlefield.iter().any(|&o| e.state.object(o).chars.name == "Pest"), "ETB Pest");
        // No creature has died yet → the end-step gate is off.
        let gate = Condition::CreatureDiedThisTurn { who: PlayerRef::Controller };
        assert!(!holds_for_source(&e.state, &gate, PlayerId(0), None));
        // Once a creature has died under P0's control this turn (the death SBA increments the flag,
        // mirroring the graveyard-leave counter), the gate holds.
        let _ = grp::FOREST;
        e.state.player_mut(PlayerId(0)).creatures_died_this_turn = 1;
        assert!(holds_for_source(&e.state, &gate, PlayerId(0), None), "creature-died gate now holds");
    }

    /// Integration (real turn engine): the end-step draw fires when a creature died under your
    /// control this turn, and is withheld otherwise — proving the `intervening_if: true` condition is
    /// evaluated (CR 603.4) as the trigger goes on the stack / resolves, not silently ignored.
    #[test]
    fn essenceknit_end_step_draw_fires_only_when_a_creature_died() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
        use crate::basics::{Phase, Zone};
        use crate::cards::{build_game, grp};
        use crate::ids::PlayerId;
        use crate::priority::Engine;

        #[derive(Clone)]
        struct PassAgent;
        impl Agent for PassAgent {
            fn decide(&mut self, _v: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        // Drive P0's end step; `died` = whether a creature died under P0's control this turn.
        // Returns cards drawn (hand delta).
        let run = |died: bool| -> usize {
            let mut state = build_game(1, &[&[grp::FOREST, grp::FOREST], &[]]);
            let scholar = {
                let c = state.card_db().get(ESSENCEKNIT_SCHOLAR).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            state.objects.get_mut(&scholar).unwrap().summoning_sick = false;
            if died {
                state.player_mut(PlayerId(0)).creatures_died_this_turn = 1;
            }
            state.active_player = PlayerId(0);
            state.phase = Phase::End;
            let hand_before = state.player(PlayerId(0)).hand.len();
            let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
            e.broadcast(GameEvent::PhaseBegan { turn: 1, phase: Phase::End, active: PlayerId(0) });
            e.run_agenda();
            if !e.state.stack.is_empty() {
                e.resolve_top();
            }
            e.state.player(PlayerId(0)).hand.len() - hand_before
        };

        assert_eq!(run(true), 1, "a creature died → intervening-if holds → draw one");
        assert_eq!(run(false), 0, "no creature died → intervening-if fails → no draw");
    }
}

#[cfg(test)]
mod repro_tests {
    use super::*;
    use crate::agent::{CastVariant, RandomAgent};
    use crate::basics::{CardType, Phase, Zone};
    use crate::cards::grp;
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    // User bug report: Essenceknit Scholar ({B}{B/G}{G}) AND Foolish Fate ({2}{B}) cast in one
    // turn off 5 lands (6 mana total). Repro: 2 Swamp + 3 Forest, cast Scholar, count taps and
    // check whether Foolish Fate is still payable.
    #[test]
    fn five_lands_cannot_cast_both_scholar_and_foolish_fate() {
        let mut db = crate::cards::starter_db();
        register(&mut db);
        crate::cards::sos::foolish_fate::register(&mut db);
        let mut state = GameState::new(2, 7);
        state.set_card_db(Arc::new(db));
        let scholar = {
            let c = state.card_db().get(ESSENCEKNIT_SCHOLAR).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let fate = {
            let c = state.card_db().get(crate::cards::sos::foolish_fate::FOOLISH_FATE).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        for _ in 0..2 {
            let s = state.card_db().get(grp::SWAMP).unwrap().chars.clone();
            state.add_card(PlayerId(0), s, Zone::Battlefield);
        }
        for _ in 0..3 {
            let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), f, Zone::Battlefield);
        }
        // An opposing creature so Foolish Fate has a target.
        let bear = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        state.add_card(PlayerId(1), bear, Zone::Battlefield);

        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.state.phase = Phase::PrecombatMain;

        e.cast_spell(PlayerId(0), scholar, CastVariant::Normal);
        let tapped: usize = e.state.player(PlayerId(0)).battlefield.iter()
            .filter(|&&o| { let x = e.state.object(o); x.status.tapped && x.chars.card_types.contains(&CardType::Land) })
            .count();
        assert_eq!(tapped, 3, "Scholar costs 3 — exactly 3 lands must be tapped, got {tapped}");
        e.resolve_top();
        assert!(
            e.state.player(PlayerId(0)).mana_pool.amounts.is_empty(),
            "no mana may float after an exact-cost hybrid cast: {:?}",
            e.state.player(PlayerId(0)).mana_pool
        );

        // Now only 2 untapped lands remain (both Forests at best): {2}{B} must NOT be payable.
        let cost = e.state.object(fate).chars.mana_cost.clone().unwrap();
        let can = crate::mana::can_pay(&e.state, PlayerId(0), &cost);
        assert!(!can, "Foolish Fate ({{2}}{{B}}) must NOT be payable off 2 untapped lands");
    }
}
