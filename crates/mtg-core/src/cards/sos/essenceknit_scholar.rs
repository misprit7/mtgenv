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
                effect: Effect::CreateToken { spec: pest_token(), count: ValueExpr::Fixed(1), controller: PlayerRef::Controller },
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
}
