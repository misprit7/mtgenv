//! Hop to It — `{2}{W}` Sorcery (first printed BLB; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Create three 1/1 white Rabbit creature tokens."
//!
//! **Fully implemented** — a `CreateToken` of three vanilla 1/1 white Rabbit tokens.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::TokenSpec;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const HOP_TO_IT: u32 = 604;

pub fn register(db: &mut CardDb) {
    let effect = Effect::CreateToken {
        spec: TokenSpec {
            name: "Rabbit".to_string(),
            card_types: vec![CardType::Creature],
            subtypes: vec![CreatureType::Rabbit.into()],
            colors: vec![Color::White],
            power: 1,
            toughness: 1,
            keywords: vec![],
            counters: vec![],
            grp_id: 0,
        },
        count: ValueExpr::Fixed(3),
        controller: PlayerRef::Controller,
        dynamic_counters: vec![],
    };
    db.insert(
        spell(
            HOP_TO_IT,
            "Hop to It",
            CardType::Sorcery,
            Color::White,
            mana_cost(2, &[(Color::White, 1)]),
            effect,
        )
        .with_text("Create three 1/1 white Rabbit creature tokens."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::cards::build_game;
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

    #[test]
    fn hop_to_it_makes_three_rabbits() {
        let state = build_game(1, &[&[], &[]]);
        let effect = state.card_db().get(HOP_TO_IT).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let rabbits = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .filter(|&&id| {
                let c = &e.state.object(id).chars;
                c.name == "Rabbit" && c.power == Some(1) && c.toughness == Some(1)
            })
            .count();
        assert_eq!(rabbits, 3, "three 1/1 Rabbit tokens");
    }
}
