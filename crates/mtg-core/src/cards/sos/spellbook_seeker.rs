//! Spellbook Seeker // Careful Study — `{3}{U}` Creature — Bird Wizard 3/3 // `{U}` Sorcery
//! (first printed SOS). A **Prepare** DFC (enters prepared).
//!
//! Front: "This creature enters prepared."  Back (Careful Study): "Draw two cards, then discard two
//! cards."
//!
//! **Fully implemented** — enters-prepared via [`helpers::enters_prepared`]; the back draws two then
//! the controller discards two.

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

pub const SPELLBOOK_SEEKER: u32 = 385;
pub const CAREFUL_STUDY: u32 = 9712;

pub fn register(db: &mut CardDb) {
    let careful_study = Effect::Sequence(vec![
        Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(2) },
        Effect::Discard { who: PlayerRef::Controller, count: ValueExpr::Fixed(2) },
    ]);
    db.insert(
        spell(CAREFUL_STUDY, "Careful Study", CardType::Sorcery, Color::Blue, mana_cost(0, &[(Color::Blue, 1)]), careful_study)
            .with_text("Draw two cards, then discard two cards."),
    );
    let mut front = creature(
        SPELLBOOK_SEEKER,
        "Spellbook Seeker",
        &[CreatureType::Bird, CreatureType::Wizard],
        Color::Blue,
        mana_cost(3, &[(Color::Blue, 1)]),
        3,
        3,
        helpers::enters_prepared(CAREFUL_STUDY),
    );
    front.text = "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Careful Study {U} Sorcery — Draw two cards, then discard two cards.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::cards::{build_game, grp};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[test]
    fn careful_study_draws_two_then_discards_two() {
        // Library has two cards to draw; hand starts with two more so a discard of two is possible.
        let mut state = build_game(1, &[&[grp::ISLAND, grp::ISLAND], &[]]);
        for _ in 0..2 {
            let c = state.card_db().get(grp::ISLAND).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, crate::basics::Zone::Hand);
        }
        let effect = state.card_db().get(CAREFUL_STUDY).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        // Drew 2 (library → empty), discarded 2 (→ graveyard). Net hand size returns to 2.
        assert!(e.state.player(PlayerId(0)).library.is_empty(), "drew both library cards");
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 2, "discarded two to the graveyard");
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 2, "hand net unchanged (drew 2, discarded 2)");
    }
}
