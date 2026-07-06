//! Fracture — `{W}{B}` Instant (first printed STX; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Destroy target artifact, enchantment, or planeswalker."
//!
//! **Fully implemented** — a single-target `Destroy` restricted to an artifact/enchantment/planeswalker.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const FRACTURE: u32 = 609;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Destroy {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Permanent(CardFilter::AnyOf(vec![
                CardFilter::HasCardType(CardType::Artifact),
                CardFilter::HasCardType(CardType::Enchantment),
                CardFilter::HasCardType(CardType::Planeswalker),
            ])),
            min: 1,
            max: 1,
            distinct: true,
        }),
    };
    let mut def = spell(
        FRACTURE,
        "Fracture",
        CardType::Instant,
        Color::White,
        mana_cost(0, &[(Color::White, 1), (Color::Black, 1)]),
        effect,
    )
    .with_text("Destroy target artifact, enchantment, or planeswalker.");
    def.chars.colors = vec![Color::White, Color::Black];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Target, Zone};
    use crate::cards::build_game;
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use crate::state::Characteristics;

    #[derive(Clone)]
    struct Passive;
    impl Agent for Passive {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    #[test]
    fn fracture_destroys_an_enchantment() {
        let mut state = build_game(1, &[&[], &[]]);
        // A bare enchantment permanent on P1's battlefield.
        let ench = state.add_card(
            PlayerId(1),
            Characteristics {
                name: "Test Enchantment".to_string(),
                card_types: vec![CardType::Enchantment],
                grp_id: 8001,
                ..Default::default()
            },
            Zone::Battlefield,
        );
        let effect = state.card_db().get(FRACTURE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(ench)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert!(!e.state.player(PlayerId(1)).battlefield.contains(&ench), "enchantment destroyed");
    }
}
