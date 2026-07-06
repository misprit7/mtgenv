//! Brain Freeze — `{1}{U}` Instant (first printed SCG; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Target player mills three cards.
//! Storm (When you cast this spell, copy it for each spell cast before it this turn. You may choose
//! new targets for the copies.)"
//!
//! **Fully implemented** — a `TargetPlayer` + `Mill 3` (the milled player read via `ChosenTarget(0)`),
//! plus **storm** (`Triggered { SelfCast → CopySpellOnStack{ SpellsCastThisTurn − 1, new targets } }`).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::PlayerFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const BRAIN_FREEZE: u32 = 611;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::TargetPlayer(PlayerFilter::Any),
        Effect::Mill { who: PlayerRef::ChosenTarget(0), count: ValueExpr::Fixed(3) },
    ]);
    let mut def = spell(
        BRAIN_FREEZE,
        "Brain Freeze",
        CardType::Instant,
        Color::Blue,
        mana_cost(1, &[(Color::Blue, 1)]),
        effect,
    )
    .with_text("Target player mills three cards.\nStorm (When you cast this spell, copy it for each spell cast before it this turn. You may choose new targets for the copies.)");
    def.abilities.push(Ability::Triggered {
        event: EventPattern::SelfCast,
        condition: None,
        intervening_if: false,
        effect: Effect::CopySpellOnStack {
            what: EffectTarget::Triggering,
            count: ValueExpr::Sum(
                Box::new(ValueExpr::SpellsCastThisTurn { who: PlayerRef::Controller }),
                Box::new(ValueExpr::Fixed(-1)),
            ),
            choose_new_targets: true,
        },
    });
    db.insert(def);
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

    #[test]
    fn brain_freeze_mills_three() {
        let mut state = build_game(1, &[&[], &[]]);
        for _ in 0..5 {
            state.add_card(PlayerId(1), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        }
        let effect = state.card_db().get(BRAIN_FREEZE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Player(PlayerId(1))], ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(1)).graveyard.len(), 3, "milled three cards");
        assert_eq!(e.state.player(PlayerId(1)).library.len(), 2, "two left in library");
    }
}
