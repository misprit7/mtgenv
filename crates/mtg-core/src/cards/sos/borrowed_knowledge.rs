//! Borrowed Knowledge — `{2}{R}{W}` Sorcery.
//!
//! Oracle: "Choose one —
//! • Discard your hand, then draw cards equal to the number of cards in target opponent's hand.
//! • Discard your hand, then draw cards equal to the number of cards discarded this way."
//!
//! **Fully implemented** — a `Modal{ choose one }`. Both modes first discard your whole hand
//! (`Discard{ Controller, HandSize{Controller} }`); they differ only in the draw count:
//! - **Mode 1** targets an opponent and draws `HandSize{ ChosenTarget(0) }` (a `TargetPlayer(Opponent)`
//!   slot bound by `PlayerRef::ChosenTarget(0)`, the End-of-the-Hunt idiom).
//! - **Mode 2** draws [`ValueExpr::DiscardedThisResolution`] — the count captured by the discard scratch
//!   this resolution ("cards discarded this way"). Since the discard runs (imperatively) before the draw
//!   materializes, the count is populated in time.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::PlayerFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, Mode};

/// grp id (per-set ids live near their cards).
pub const BORROWED_KNOWLEDGE: u32 = 431;

/// "Discard your hand" — discard exactly your current hand size.
fn discard_hand() -> Effect {
    Effect::Discard { who: PlayerRef::Controller, count: ValueExpr::HandSize { who: PlayerRef::Controller } }
}

pub fn register(db: &mut CardDb) {
    let effect = Effect::Modal {
        modes: vec![
            Mode {
                label: "Discard your hand, then draw cards equal to the number of cards in target opponent's hand".to_string(),
                effect: Effect::Sequence(vec![
                    Effect::TargetPlayer(PlayerFilter::Opponent),
                    discard_hand(),
                    Effect::Draw {
                        who: PlayerRef::Controller,
                        count: ValueExpr::HandSize { who: PlayerRef::ChosenTarget(0) },
                    },
                ]),
            },
            Mode {
                label: "Discard your hand, then draw cards equal to the number of cards discarded this way".to_string(),
                effect: Effect::Sequence(vec![
                    discard_hand(),
                    Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::DiscardedThisResolution },
                ]),
            },
        ],
        min: 1,
        max: 1,
        allow_repeat: false,
    };
    let mut def = spell(
        BORROWED_KNOWLEDGE,
        "Borrowed Knowledge",
        CardType::Sorcery,
        Color::Red,
        mana_cost(2, &[(Color::Red, 1), (Color::White, 1)]),
        effect,
    )
    .with_text("Choose one —\n• Discard your hand, then draw cards equal to the number of cards in target opponent's hand.\n• Discard your hand, then draw cards equal to the number of cards discarded this way.");
    def.chars.colors = vec![Color::Red, Color::White];
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView};
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
    fn borrowed_knowledge_shape() {
        let db = db_with_card();
        let def = db.get(BORROWED_KNOWLEDGE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.colors, vec![Color::Red, Color::White]);
        assert!(def.fully_implemented);
        let Some(Effect::Modal { modes, .. }) = def.spell_effect() else { panic!("modal") };
        assert_eq!(modes.len(), 2);
    }

    /// Picks mode `mode` (0 or 1); targets P1 for mode 0. All other decisions default (discard fills
    /// from the front of the hand, which is fine since we discard the whole hand anyway).
    #[derive(Clone)]
    struct BkAgent {
        mode: u32,
    }
    impl Agent for BkAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseModes { .. } => DecisionResponse::Indices(vec![self.mode]),
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let idx = slots[0]
                        .legal
                        .iter()
                        .position(|t| *t == Target::Player(PlayerId(1)))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
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

    /// P0 has Borrowed Knowledge + `p0_hand` extra cards in hand; P1 has `p1_hand` cards. Lands to pay
    /// {2}{R}{W}. Returns engine + the spell id.
    fn setup(mode: u32, p0_extra_hand: usize, p1_hand: usize) -> Engine {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        let spell = {
            let c = state.card_db().get(BORROWED_KNOWLEDGE).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        for _ in 0..p0_extra_hand {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand);
        }
        for _ in 0..p1_hand {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(1), c, Zone::Hand);
        }
        // Ensure P0's library has cards to draw.
        for _ in 0..20 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library);
        }
        for g in [grp::MOUNTAIN, grp::PLAINS, grp::MOUNTAIN, grp::PLAINS] {
            let c = state.card_db().get(g).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield); // {2}{R}{W}
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(BkAgent { mode }), Box::new(BkAgent { mode })]);
        e.cast_spell(PlayerId(0), spell, CastVariant::Normal);
        e
    }

    /// Cards in P0's hand that are NOT the Borrowed Knowledge spell (it's on the stack/gy after cast).
    fn p0_hand(e: &Engine) -> usize {
        e.state.player(PlayerId(0)).hand.len()
    }

    /// Mode 1: discard your hand (3 extra cards), draw = P1's hand (5). Net hand = 5.
    #[test]
    fn mode1_draws_target_opponents_hand_count() {
        let mut e = setup(0, 3, 5);
        drive(&mut e);
        assert_eq!(p0_hand(&e), 5, "drew 5 = P1's hand size (discarded 3 first)");
    }

    /// Mode 2: discard your hand (4 extra cards), draw = number discarded this way (4). Net hand = 4.
    #[test]
    fn mode2_draws_number_discarded() {
        let mut e = setup(1, 4, 2);
        drive(&mut e);
        assert_eq!(p0_hand(&e), 4, "drew 4 = the 4 cards discarded this way");
    }

    /// Mode 2 with an empty hand: discard 0, draw 0. Net hand = 0.
    #[test]
    fn mode2_empty_hand_draws_nothing() {
        let mut e = setup(1, 0, 2);
        drive(&mut e);
        assert_eq!(p0_hand(&e), 0, "discarded 0, drew 0");
    }
}
