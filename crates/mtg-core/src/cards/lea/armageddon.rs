//! Armageddon — `{3}{W}` Sorcery (first printed LEA; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Destroy all lands."
//!
//! **Fully implemented** — a symmetric mass-destroy: `ForEach` over every land on the battlefield
//! (both players), body `Destroy { Each }`. Mirrors the shipped `vicious_rivalry` mass-destroy idiom.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const ARMAGEDDON: u32 = 601;

pub fn register(db: &mut CardDb) {
    let effect = Effect::ForEach {
        selector: SelectSpec {
            zone: Zone::Battlefield,
            filter: CardFilter::HasCardType(CardType::Land),
            chooser: PlayerRef::EachPlayer,
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(999),
        },
        body: Box::new(Effect::Destroy { what: EffectTarget::Each }),
    };
    db.insert(
        spell(
            ARMAGEDDON,
            "Armageddon",
            CardType::Sorcery,
            Color::White,
            mana_cost(3, &[(Color::White, 1)]),
            effect,
        )
        .with_text("Destroy all lands."),
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
    fn armageddon_ir_and_text() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ARMAGEDDON).unwrap();
        assert!(def.fully_implemented);
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.text, "Destroy all lands.");
    }

    /// Behaviour: both players' lands are destroyed; creatures survive.
    #[test]
    fn armageddon_destroys_every_land() {
        let mut state = build_game(1, &[&[], &[]]);
        // P0: two Forests + a Grizzly Bears; P1: a Mountain.
        let f0 = state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Battlefield);
        let f1 = state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Battlefield);
        let bear = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let m0 = state.add_card(PlayerId(1), state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(ARMAGEDDON).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        for land in [f0, f1, m0] {
            assert!(!e.state.player(PlayerId(0)).battlefield.contains(&land) && !e.state.player(PlayerId(1)).battlefield.contains(&land), "land destroyed");
        }
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&bear), "creature survives Armageddon");
    }
}
