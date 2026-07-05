//! Mathemagics — `{X}{X}{U}{U}` Sorcery.
//!
//! Oracle: "Target player draws 2ˣ cards. (2⁰ = 1, 2¹ = 2, 2² = 4, 2³ = 8, …)"
//!
//! **Fully implemented** — NOT a Native (the ledger's "2^X exponential ⇒ Native" tag overstated it). The
//! exponential is a pure value: [`Effect::TargetPlayer`] + [`Effect::Draw`] of [`ValueExpr::Pow2`] of the
//! chosen `{X}`. `Pow2` is a generic base-2 exponent node (clamped so it can't overflow), so any future
//! "2ˣ" card reuses it. `{X}{X}` = two pips (`mc.x = 2`); both read the single chosen X.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::target::PlayerFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const MATHEMAGICS: u32 = 455;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Sequence(vec![
        Effect::TargetPlayer(PlayerFilter::Any),
        Effect::Draw {
            who: PlayerRef::ChosenTarget(0),
            count: ValueExpr::Pow2(Box::new(ValueExpr::X)),
        },
    ]);
    let mut mc = mana_cost(0, &[(Color::Blue, 2)]);
    mc.x = 2; // `{X}{X}{U}{U}` — two `{X}` pips (both read the single chosen X value).
    let def = spell(MATHEMAGICS, "Mathemagics", CardType::Sorcery, Color::Blue, mc, effect)
        .with_text("Target player draws 2ˣ cards. (2⁰ = 1, 2¹ = 2, 2² = 4, 2³ = 8, 2⁴ = 16, 2⁵ = 32, and so on.)");
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, CastVariant, DecisionRequest, DecisionResponse, PlayerView, RandomAgent};
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
    fn mathemagics_shape() {
        let db = db_with_card();
        let def = db.get(MATHEMAGICS).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Sorcery]);
        assert_eq!(def.chars.colors, vec![Color::Blue]);
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().x, 2, "two {{X}} pips");
        assert!(def.fully_implemented);
    }

    /// Picks X and aims the "target player" at self.
    struct MathAgent {
        x: u32,
    }
    impl Agent for MathAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseNumber { .. } => DecisionResponse::Number(self.x as i64),
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let idx = slots[0]
                        .legal
                        .iter()
                        .position(|t| *t == Target::Player(PlayerId(0)))
                        .unwrap_or(0);
                    DecisionResponse::Pairs(vec![(0, idx as u32)])
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Cast for X=3 → draw 2³ = 8 cards.
    #[test]
    fn draws_two_to_the_x() {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db_with_card()));
        // {X}{X}{U}{U} at X=3 = {3}{3}{U}{U} = 8 mana. Give 6 Islands + 2 more Islands = 8 blue sources.
        for _ in 0..8 {
            let c = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let math = {
            let c = state.card_db().get(MATHEMAGICS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        for _ in 0..10 {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Library);
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(MathAgent { x: 3 }), Box::new(RandomAgent::new(1))]);
        let hand_before = e.state.player(PlayerId(0)).hand.len(); // 1 (Mathemagics)
        e.cast_spell(PlayerId(0), math, CastVariant::Normal);
        e.resolve_top();
        // Cast removed Mathemagics (→0), drew 2³ = 8.
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand_before - 1 + 8, "drew 2^3 = 8");
    }
}
