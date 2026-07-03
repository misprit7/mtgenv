//! Quick Study — `{2}{U}` Instant (first printed WOE; reprinted in SOS).
//!
//! Oracle: "Draw two cards."
//!
//! **Fully implemented** — a vanilla draw spell.

use crate::basics::{CardType, Color};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const QUICK_STUDY: u32 = 274;

pub fn register(db: &mut CardDb) {
    db.insert(
        spell(
            QUICK_STUDY,
            "Quick Study",
            CardType::Instant,
            Color::Blue,
            mana_cost(2, &[(Color::Blue, 1)]),
            Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(2) },
        )
        .with_text("Draw two cards."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::Zone;
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[test]
    fn quick_study_draws_two() {
        let lib: Vec<u32> = std::iter::repeat(grp::FOREST).take(3).collect();
        let mut state = build_game(1, &[&lib, &[]]);
        let effect = state.card_db().get(QUICK_STUDY).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        let before = e.state.players[0].hand.len();
        e.resolve_effect(&effect, &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.players[0].hand.len(), before + 2);
    }
}
