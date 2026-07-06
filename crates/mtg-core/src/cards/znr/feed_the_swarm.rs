//! Feed the Swarm — `{1}{B}` Sorcery (first printed ZNR; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Destroy target creature or enchantment an opponent controls. You lose life equal to that
//! permanent's mana value."
//!
//! **Fully implemented** — `Destroy` a creature/enchantment an opponent controls, then `LoseLife`
//! equal to `ManaValueOfTarget(0)` (the destroyed permanent's mana value; its mana cost persists in the
//! graveyard, so the value reads correctly after the destroy).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const FEED_THE_SWARM: u32 = 612;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::Destroy {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Permanent(CardFilter::All(vec![
                    CardFilter::AnyOf(vec![
                        CardFilter::HasCardType(CardType::Creature),
                        CardFilter::HasCardType(CardType::Enchantment),
                    ]),
                    CardFilter::ControlledBy(PlayerRef::Opponent),
                ])),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
        Effect::LoseLife { who: PlayerRef::Controller, amount: ValueExpr::ManaValueOfTarget(0) },
    ]);
    db.insert(
        spell(
            FEED_THE_SWARM,
            "Feed the Swarm",
            CardType::Sorcery,
            Color::Black,
            mana_cost(1, &[(Color::Black, 1)]),
            effect,
        )
        .with_text("Destroy target creature or enchantment an opponent controls. You lose life equal to that permanent's mana value."),
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

    /// Destroys an opponent's Hill Giant (MV 4) and the caster loses 4 life.
    #[test]
    fn destroys_and_loses_life_equal_to_mv() {
        let mut state = build_game(1, &[&[], &[]]);
        let giant = state.add_card(PlayerId(1), state.card_db().get(grp::HILL_GIANT).unwrap().chars.clone(), Zone::Battlefield);
        let life_before = state.player(PlayerId(0)).life;
        let mv = state.object(giant).chars.mana_value();
        let effect = state.card_db().get(FEED_THE_SWARM).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(giant)], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert!(!e.state.player(PlayerId(1)).battlefield.contains(&giant), "creature destroyed");
        assert_eq!(e.state.player(PlayerId(0)).life, life_before - mv as i32, "lost life = its mana value");
    }
}
