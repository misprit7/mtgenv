//! Zombify — `{3}{B}` Sorcery (first printed ODY; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Return target creature card from your graveyard to the battlefield."
//!
//! **Fully implemented** — a `MoveZone` reanimation of a target creature card from your graveyard
//! onto the battlefield (untapped).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const ZOMBIFY: u32 = 616;

pub fn register(db: &mut CardDb) {
    let effect = Effect::MoveZone {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::CardInZone {
                zone: Zone::Graveyard,
                filter: CardFilter::All(vec![
                    CardFilter::ControlledBy(PlayerRef::Controller),
                    CardFilter::HasCardType(CardType::Creature),
                ]),
            },
            min: 1,
            max: 1,
            distinct: true,
        }),
        to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
        tapped: false,
    };
    db.insert(
        spell(
            ZOMBIFY,
            "Zombify",
            CardType::Sorcery,
            Color::Black,
            mana_cost(3, &[(Color::Black, 1)]),
            effect,
        )
        .with_text("Return target creature card from your graveyard to the battlefield."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::Target;
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
    fn zombify_reanimates_from_your_graveyard() {
        let mut state = build_game(1, &[&[], &[]]);
        let dead = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Graveyard);
        let effect = state.card_db().get(ZOMBIFY).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(dead)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&dead), "reanimated onto the battlefield");
        assert!(!e.state.player(PlayerId(0)).graveyard.contains(&dead), "no longer in the graveyard");
    }
}
