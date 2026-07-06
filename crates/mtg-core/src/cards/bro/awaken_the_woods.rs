//! Awaken the Woods — `{X}{G}{G}` Sorcery (first printed BRO; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Create X 1/1 green Forest Dryad land creature tokens."
//!
//! **Fully implemented** — a `CreateToken` of X tokens that are **both** Land and Creature (a Forest
//! Dryad): 1/1 green, with the Forest land type (so they tap for `{G}` intrinsically, CR 305.6) and
//! subject to summoning sickness (they're creatures). `count = X` chosen at cast.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::TokenSpec;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::{CreatureType, LandType};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const AWAKEN_THE_WOODS: u32 = 633;

pub fn register(db: &mut CardDb) {
    let effect = Effect::CreateToken {
        spec: TokenSpec {
            name: "Forest Dryad".to_string(),
            card_types: vec![CardType::Land, CardType::Creature],
            subtypes: vec![LandType::Forest.into(), CreatureType::Dryad.into()],
            colors: vec![Color::Green],
            power: 1,
            toughness: 1,
            keywords: vec![],
            counters: vec![],
            grp_id: 0,
        },
        count: ValueExpr::X,
        controller: PlayerRef::Controller,
        dynamic_counters: vec![],
    };
    db.insert(
        spell(
            AWAKEN_THE_WOODS,
            "Awaken the Woods",
            CardType::Sorcery,
            Color::Green,
            mana_cost(0, &[(Color::Green, 2)]),
            effect,
        )
        .with_text("Create X 1/1 green Forest Dryad land creature tokens. (They're affected by summoning sickness.)"),
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

    /// X=2: two 1/1 Forest Dryad tokens that are both Land and Creature.
    #[test]
    fn makes_x_land_creature_tokens() {
        let state = build_game(1, &[&[], &[]]);
        let effect = state.card_db().get(AWAKEN_THE_WOODS).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), x: Some(2), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let dryads: Vec<_> = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .filter(|&&id| e.state.object(id).chars.name == "Forest Dryad")
            .copied()
            .collect();
        assert_eq!(dryads.len(), 2, "created X=2 Forest Dryads");
        let c = &e.state.object(dryads[0]).chars;
        assert!(c.card_types.contains(&CardType::Land) && c.card_types.contains(&CardType::Creature), "both land and creature");
        assert_eq!((c.power, c.toughness), (Some(1), Some(1)), "1/1");
        assert!(e.state.object(dryads[0]).summoning_sick, "enters with summoning sickness");
    }
}
