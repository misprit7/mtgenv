//! Sheoldred's Edict — `{1}{B}` Instant (first printed ONE; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Choose one —
//! • Each opponent sacrifices a nontoken creature of their choice.
//! • Each opponent sacrifices a creature token of their choice.
//! • Each opponent sacrifices a planeswalker of their choice."
//!
//! **Fully implemented** — a `Modal` of three `Sacrifice { who: EachOpponent }` edicts. The nontoken
//! vs token distinction uses `Not(Supertype(Token))` / `Supertype(Token)` (created tokens carry the
//! Token supertype, CR 111.1).

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, Mode};
use crate::subtypes::Supertype;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const SHEOLDREDS_EDICT: u32 = 640;

fn each_opp_sacrifices(filter: CardFilter) -> Effect {
    Effect::Sacrifice {
        who: PlayerRef::EachOpponent,
        what: SelectSpec {
            zone: Zone::Battlefield,
            filter,
            chooser: PlayerRef::Opponent,
            min: ValueExpr::Fixed(1),
            max: ValueExpr::Fixed(1),
        },
    }
}

pub fn register(db: &mut CardDb) {
    let creature = CardFilter::HasCardType(CardType::Creature);
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "Each opponent sacrifices a nontoken creature of their choice".to_string(),
                effect: each_opp_sacrifices(CardFilter::All(vec![
                    creature.clone(),
                    CardFilter::Not(Box::new(CardFilter::Supertype(Supertype::Token))),
                ])),
            },
            Mode {
                label: "Each opponent sacrifices a creature token of their choice".to_string(),
                effect: each_opp_sacrifices(CardFilter::All(vec![
                    creature,
                    CardFilter::Supertype(Supertype::Token),
                ])),
            },
            Mode {
                label: "Each opponent sacrifices a planeswalker of their choice".to_string(),
                effect: each_opp_sacrifices(CardFilter::HasCardType(CardType::Planeswalker)),
            },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    db.insert(
        spell(
            SHEOLDREDS_EDICT,
            "Sheoldred's Edict",
            CardType::Instant,
            Color::Black,
            mana_cost(1, &[(Color::Black, 1)]),
            effect,
        )
        .with_text("Choose one —\n• Each opponent sacrifices a nontoken creature of their choice.\n• Each opponent sacrifices a creature token of their choice.\n• Each opponent sacrifices a planeswalker of their choice."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::effects::target::TokenSpec;
    use crate::effects::value::PlayerRef as PR;
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use crate::subtypes::CreatureType;

    #[derive(Clone)]
    struct SacFirst;
    impl Agent for SacFirst {
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

    /// Give P1 a real (nontoken) creature and an engine-created 1/1 token creature. Mode 1 (nontoken)
    /// takes the real one and spares the token; mode 2 (token) takes the token and spares the real one.
    /// Exercises that `create_token` stamps `Supertype::Token`.
    fn setup() -> (Engine, PlayerId) {
        let mut state = build_game(1, &[&[], &[]]);
        state.players[1].battlefield.clear();
        let nontoken = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let mut e = Engine::new(state, vec![Box::new(SacFirst), Box::new(SacFirst)]);
        // Create a real token via the engine so it's Token-stamped.
        e.resolve_effect(
            &Effect::CreateToken {
                spec: TokenSpec { name: "Soldier".to_string(), card_types: vec![CardType::Creature], subtypes: vec![CreatureType::Soldier.into()], colors: vec![Color::White], power: 1, toughness: 1, keywords: vec![], counters: vec![], grp_id: 0 },
                count: ValueExpr::Fixed(1),
                controller: PR::Controller,
                dynamic_counters: vec![],
            },
            &ResolutionCtx { controller: Some(PlayerId(1)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let _ = nontoken;
        (e, PlayerId(1))
    }

    #[test]
    fn mode1_takes_nontoken_spares_token() {
        let (mut e, p1) = setup();
        let effect = e.state.card_db().get(SHEOLDREDS_EDICT).unwrap().spell_effect().unwrap().clone();
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_modes: vec![0], ..Default::default() },
            WbReason::Resolve(StackId(1)),
        );
        let names: Vec<_> = e.state.player(p1).battlefield.iter().map(|&id| e.state.object(id).chars.name.clone()).collect();
        assert!(names.contains(&"Soldier".to_string()), "the token survives mode 1 (nontoken)");
        assert!(!names.contains(&"Grizzly Bears".to_string()), "the nontoken creature was sacrificed");
    }

    #[test]
    fn mode2_takes_token_spares_nontoken() {
        let (mut e, p1) = setup();
        let effect = e.state.card_db().get(SHEOLDREDS_EDICT).unwrap().spell_effect().unwrap().clone();
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_modes: vec![1], ..Default::default() },
            WbReason::Resolve(StackId(1)),
        );
        let names: Vec<_> = e.state.player(p1).battlefield.iter().map(|&id| e.state.object(id).chars.name.clone()).collect();
        assert!(names.contains(&"Grizzly Bears".to_string()), "the nontoken creature survives mode 2 (token)");
        assert!(!names.contains(&"Soldier".to_string()), "the creature token was sacrificed");
    }
}
