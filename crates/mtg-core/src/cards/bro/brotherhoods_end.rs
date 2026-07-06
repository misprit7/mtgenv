//! Brotherhood's End — `{1}{R}{R}` Sorcery (first printed BRO; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Choose one —
//! • Brotherhood's End deals 3 damage to each creature and each planeswalker.
//! • Destroy all artifacts with mana value 3 or less."
//!
//! **Fully implemented** — a `Modal` "choose one": mode 1 is a `ForEach` mass 3-damage over every
//! creature/planeswalker (`DealDamage { Each }`); mode 2 is a `ForEach` mass-destroy over artifacts of
//! mana value 3 or less. Mirrors the shipped `lorehold_charm` (Modal) + `vicious_rivalry` (mass) idioms.

use crate::basics::{CardType, Color, DamageKind, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget, Mode};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const BROTHERHOODS_END: u32 = 603;

pub fn register(db: &mut CardDb) {
    let deal_3_to_each = Effect::ForEach {
        selector: SelectSpec {
            zone: Zone::Battlefield,
            filter: CardFilter::AnyOf(vec![
                CardFilter::HasCardType(CardType::Creature),
                CardFilter::HasCardType(CardType::Planeswalker),
            ]),
            chooser: PlayerRef::EachPlayer,
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(999),
        },
        body: Box::new(Effect::DealDamage {
            amount: ValueExpr::Fixed(3),
            to: EffectTarget::Each,
            kind: DamageKind::Noncombat,
        }),
    };
    let destroy_small_artifacts = Effect::ForEach {
        selector: SelectSpec {
            zone: Zone::Battlefield,
            filter: CardFilter::All(vec![
                CardFilter::HasCardType(CardType::Artifact),
                CardFilter::ManaValue { min: None, max: Some(3) },
            ]),
            chooser: PlayerRef::EachPlayer,
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(999),
        },
        body: Box::new(Effect::Destroy { what: EffectTarget::Each }),
    };
    let effect = Effect::Modal {
        modes: vec![
            Mode { label: "Brotherhood's End deals 3 damage to each creature and each planeswalker".to_string(), effect: deal_3_to_each },
            Mode { label: "Destroy all artifacts with mana value 3 or less".to_string(), effect: destroy_small_artifacts },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    db.insert(
        spell(
            BROTHERHOODS_END,
            "Brotherhood's End",
            CardType::Sorcery,
            Color::Red,
            mana_cost(1, &[(Color::Red, 2)]),
            effect,
        )
        .with_text("Choose one —\n• Brotherhood's End deals 3 damage to each creature and each planeswalker.\n• Destroy all artifacts with mana value 3 or less."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::cards::{build_game, grp};
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
    fn brotherhoods_end_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(BROTHERHOODS_END).unwrap();
        assert!(def.fully_implemented);
        match def.spell_effect().unwrap() {
            Effect::Modal { modes, min, max, .. } => assert_eq!((modes.len(), *min, *max), (2, 1, 1)),
            o => panic!("expected Modal, got {o:?}"),
        }
    }

    /// Behaviour, mode 1: 3 damage is marked on every creature (both players'). (SBAs — which would
    /// destroy the lethally-damaged creatures — run in the full engine loop, not in `resolve_effect`;
    /// the shipped `artistic_process` mass-damage test asserts on `damage_marked` the same way.)
    #[test]
    fn mode1_deals_3_to_each_creature() {
        let mut state = build_game(1, &[&[], &[]]);
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let giant = state.add_card(PlayerId(1), state.card_db().get(grp::HILL_GIANT).unwrap().chars.clone(), Zone::Battlefield); // 3/3
        let effect = state.card_db().get(BROTHERHOODS_END).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_modes: vec![0], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.object(bear).damage_marked, 3, "3 damage marked on P0's creature");
        assert_eq!(e.state.object(giant).damage_marked, 3, "3 damage marked on P1's creature");
    }
}
