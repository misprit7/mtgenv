//! Pongify — `{U}` Instant (first printed PLC; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Destroy target creature. It can't be regenerated. Its controller creates a 3/3 green Ape
//! creature token."
//!
//! **Fully implemented** — `Destroy` a target creature, then its controller (`ControllerOfTarget(0)`,
//! snapshotted before the destroy) creates a 3/3 green Ape token. ("Can't be regenerated" is a no-op:
//! the engine models no regeneration, so nothing can be regenerated to begin with.)

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec, TokenSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const PONGIFY: u32 = 630;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Destroy {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
        Effect::CreateToken {
            spec: TokenSpec {
                name: "Ape".to_string(),
                card_types: vec![CardType::Creature],
                subtypes: vec![CreatureType::Ape.into()],
                colors: vec![Color::Green],
                power: 3,
                toughness: 3,
                keywords: vec![],
                counters: vec![],
                grp_id: 0,
            },
            count: ValueExpr::Fixed(1),
            controller: PlayerRef::ControllerOfTarget(0),
            dynamic_counters: vec![],
        },
    ]);
    db.insert(
        spell(
            PONGIFY,
            "Pongify",
            CardType::Instant,
            Color::Blue,
            mana_cost(0, &[(Color::Blue, 1)]),
            effect,
        )
        .with_text("Destroy target creature. It can't be regenerated. Its controller creates a 3/3 green Ape creature token."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
    use crate::basics::{Target, Zone};
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

    /// Destroys P1's creature and P1 (its controller) gets the 3/3 Ape.
    #[test]
    fn destroys_then_owner_gets_an_ape() {
        let mut state = build_game(1, &[&[], &[]]);
        let victim = state.add_card(PlayerId(1), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let effect = state.card_db().get(PONGIFY).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(victim)],
                // Snapshot the victim's controller (the real pipeline captures this at resolution start).
                target_controllers: vec![Some(PlayerId(1))],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert!(!e.state.player(PlayerId(1)).battlefield.contains(&victim), "target creature destroyed");
        let apes = e
            .state
            .player(PlayerId(1))
            .battlefield
            .iter()
            .filter(|&&id| { let c = &e.state.object(id).chars; c.name == "Ape" && c.power == Some(3) })
            .count();
        assert_eq!(apes, 1, "the destroyed creature's controller (P1) got a 3/3 Ape");
    }
}
