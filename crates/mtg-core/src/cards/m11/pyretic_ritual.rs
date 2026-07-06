//! Pyretic Ritual — `{1}{R}` Instant (first printed M11; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Add {R}{R}{R}."
//!
//! **Fully implemented** — a ritual: its spell effect is `AddMana` of three red into the caster's pool.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::ManaSpec;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const PYRETIC_RITUAL: u32 = 602;

pub fn register(db: &mut CardDb) {
    let effect = Effect::AddMana {
        who: PlayerRef::Controller,
        mana: ManaSpec {
            produces: vec![(Color::Red, ValueExpr::Fixed(3))],
            any_color: None,
            restriction: None,
        },
    };
    db.insert(
        spell(
            PYRETIC_RITUAL,
            "Pyretic Ritual",
            CardType::Instant,
            Color::Red,
            mana_cost(1, &[(Color::Red, 1)]),
            effect,
        )
        .with_text("Add {R}{R}{R}."),
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
    fn pyretic_ritual_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PYRETIC_RITUAL).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.chars.card_types, vec![CardType::Instant]);
    }

    /// Behaviour: resolving adds three red mana to the caster's pool.
    #[test]
    fn pyretic_ritual_adds_three_red() {
        let state = build_game(1, &[&[], &[]]);
        let effect = state.card_db().get(PYRETIC_RITUAL).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(
            e.state.player(PlayerId(0)).mana_pool.amounts.get(&Color::Red).copied().unwrap_or(0),
            3,
            "added three red mana"
        );
    }
}
