//! Reprieve — `{1}{W}` Instant (first printed LTR; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "Return target spell to its owner's hand. Draw a card."
//!
//! **Fully implemented** over the new `Effect::ReturnSpellToHand` cap: bounce a target spell off the
//! stack to its owner's hand (not a counter — a can't-be-countered spell is still returned), then draw.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const REPRIEVE: u32 = 635;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::ReturnSpellToHand {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::StackObject(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
        Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
    ]);
    db.insert(
        spell(
            REPRIEVE,
            "Reprieve",
            CardType::Instant,
            Color::White,
            mana_cost(1, &[(Color::White, 1)]),
            effect,
        )
        .with_text("Return target spell to its owner's hand.\nDraw a card."),
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
    use crate::stack::{StackObject, StackObjectKind};

    #[derive(Clone)]
    struct Passive;
    impl Agent for Passive {
        fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
            DecisionResponse::Pass
        }
    }

    /// A Lightning Bolt cast by P1 is on the stack; Reprieve returns it to P1's hand and P0 draws.
    #[test]
    fn returns_a_spell_to_owners_hand_and_draws() {
        let mut state = build_game(1, &[&[], &[]]);
        // P0's library has a card to draw.
        state.add_card(PlayerId(0), state.card_db().get(grp::FOREST).unwrap().chars.clone(), Zone::Library);
        // A Lightning Bolt object owned by P1, put on the stack as a spell.
        let bolt = state.add_card(PlayerId(1), state.card_db().get(grp::LIGHTNING_BOLT).unwrap().chars.clone(), Zone::Stack);
        let sid = state.mint_stack();
        state.stack.push(StackObject {
            id: sid,
            controller: PlayerId(1),
            source: Some(bolt),
            kind: StackObjectKind::Spell(bolt),
            targets: vec![],
            x: None,
            modes: Vec::new(),
        });
        let effect = state.card_db().get(REPRIEVE).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Stack(sid)], ..Default::default() },
            WbReason::Resolve(StackId(999)),
        );
        assert!(e.state.stack.items.is_empty(), "the bolt left the stack");
        assert!(e.state.player(PlayerId(1)).hand.contains(&bolt), "returned to its owner's (P1's) hand");
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 1, "P0 drew a card");
    }
}
