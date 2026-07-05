//! Mind Roots — `{1}{B}{G}` Sorcery.
//!
//! Oracle: "Target player discards two cards. Put up to one land card discarded this way onto the
//! battlefield tapped under your control."
//!
//! **Fully implemented** — `Sequence[ TargetPlayer, Discard 2, PutDiscardedOntoBattlefield ]`. The
//! discard records the two discarded cards in the per-resolution scratch; the new
//! `Effect::PutDiscardedOntoBattlefield{ filter: Land, max: 1 }` then lets the caster put up to one land
//! among "the cards discarded this way" onto the battlefield tapped under their own control (owner
//! unchanged — the discarding player still owns it).

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::target::PlayerFilter;
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const MIND_ROOTS: u32 = 439;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::TargetPlayer(PlayerFilter::Any),
        Effect::Discard { who: PlayerRef::ChosenTarget(0), count: ValueExpr::Fixed(2) },
        Effect::PutDiscardedOntoBattlefield {
            filter: CardFilter::HasCardType(CardType::Land),
            max: 1,
        },
    ]);
    let mut def = spell(
        MIND_ROOTS,
        "Mind Roots",
        CardType::Sorcery,
        Color::Black,
        mana_cost(1, &[(Color::Black, 1), (Color::Green, 1)]),
        effect,
    )
    .with_text("Target player discards two cards. Put up to one land card discarded this way onto the battlefield tapped under your control.");
    def.chars.colors = vec![Color::Black, Color::Green];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView, SelectReason};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::{grp, starter_db};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    fn db_with_card() -> CardDb {
        let mut db = starter_db();
        register(&mut db);
        db
    }

    #[test]
    fn mind_roots_shape() {
        let db = db_with_card();
        let def = db.get(MIND_ROOTS).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.colors, vec![Color::Black, Color::Green]);
        assert!(def.fully_implemented);
    }

    /// Targets P1 for the discard; when P1 discards, discards from the front of hand; when P0 is offered
    /// the land drop, takes the first (index 0).
    #[derive(Clone)]
    struct MrAgent;
    impl Agent for MrAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let idx = slots[0]
                        .legal
                        .iter()
                        .position(|t| *t == Target::Player(PlayerId(1)))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                // The land-drop selection (P0 picks the land) — take the first offered.
                DecisionRequest::SelectCards { reason: SelectReason::Generic, from, .. } if !from.is_empty() => {
                    DecisionResponse::Indices(vec![0])
                }
                // P1's discard: front of hand.
                DecisionRequest::SelectCards { from, min, .. } => {
                    let n = (*min as usize).min(from.len());
                    DecisionResponse::Indices((0..n as u32).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn drive(e: &mut Engine) {
        loop {
            e.run_agenda();
            if e.state.stack.items.is_empty() {
                break;
            }
            e.resolve_top();
        }
    }

    /// P1 has a Forest + two Grizzly Bears in hand; Mind Roots makes P1 discard two (the two front
    /// cards), and P0 puts the discarded Forest onto the battlefield tapped under P0's control.
    #[test]
    fn discards_two_then_puts_a_discarded_land_under_your_control() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let mr = {
            let c = state.card_db().get(MIND_ROOTS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        // P1's hand ordered so the front two discards are a Forest + a Bears.
        let forest = {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Hand)
        };
        for _ in 0..2 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Hand);
        }
        for g in [grp::SWAMP, grp::FOREST, grp::SWAMP] {
            let c = state.card_db().get(g).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield); // {1}{B}{G}
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(MrAgent), Box::new(MrAgent)]);
        e.cast_spell(PlayerId(0), mr, CastVariant::Normal);
        drive(&mut e);
        // P1 discarded two cards (Forest + one Bears) → its graveyard held them; the Forest was then
        // pulled onto P0's battlefield tapped.
        assert!(e.state.player(PlayerId(0)).battlefield.contains(&forest), "the discarded Forest is under P0's control");
        assert_eq!(e.state.object(forest).controller, PlayerId(0), "P0 controls it");
        assert_eq!(e.state.object(forest).owner, PlayerId(1), "P1 still owns it");
        assert!(e.state.object(forest).status.tapped, "it entered tapped");
    }
}
