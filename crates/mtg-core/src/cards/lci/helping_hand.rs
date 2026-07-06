//! Helping Hand — `{W}` Sorcery (first printed LCI; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Return target creature card with mana value 3 or less from your graveyard to the
//! battlefield tapped."
//!
//! **Fully implemented** — a `MoveZone` reanimation of a target creature card (mana value 3 or less)
//! from your graveyard onto the battlefield **tapped**.

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const HELPING_HAND: u32 = 622;

pub fn register(db: &mut CardDb) {
    let effect = Effect::MoveZone {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::CardInZone {
                zone: Zone::Graveyard,
                filter: CardFilter::All(vec![
                    CardFilter::ControlledBy(PlayerRef::Controller),
                    CardFilter::HasCardType(CardType::Creature),
                    CardFilter::ManaValue { min: None, max: Some(3) },
                ]),
            },
            min: 1,
            max: 1,
            distinct: true,
        }),
        to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
        tapped: true,
    };
    db.insert(
        spell(
            HELPING_HAND,
            "Helping Hand",
            CardType::Sorcery,
            Color::White,
            mana_cost(0, &[(Color::White, 1)]),
            effect,
        )
        .with_text("Return target creature card with mana value 3 or less from your graveyard to the battlefield tapped."),
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
    fn helping_hand_reanimates_tapped() {
        let mut state = build_game(1, &[&[], &[]]);
        let dead = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Graveyard); // MV 2
        let effect = state.card_db().get(HELPING_HAND).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(dead)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&dead), "reanimated");
        assert!(e.state.object(dead).status.tapped, "enters tapped");
    }
}
