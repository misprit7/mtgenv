//! Landscape Painter // Vibrant Idea — `{1}{U}` Creature — Merfolk Wizard 2/1 // `{4}{U}` Sorcery
//! (first printed SOS). A **Prepare** DFC (enters prepared).
//!
//! Front: "This creature enters prepared."  Back (Vibrant Idea): "Draw two cards."
//!
//! **Fully implemented** — enters-prepared via [`helpers::enters_prepared`]; the back face draws two.

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

pub const LANDSCAPE_PAINTER: u32 = 378;
pub const VIBRANT_IDEA: u32 = 9705;

pub fn register(db: &mut CardDb) {
    db.insert(
        spell(
            VIBRANT_IDEA,
            "Vibrant Idea",
            CardType::Sorcery,
            Color::Blue,
            mana_cost(4, &[(Color::Blue, 1)]),
            Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(2) },
        )
        .with_text("Draw two cards."),
    );
    let mut front = creature(
        LANDSCAPE_PAINTER,
        "Landscape Painter",
        &[CreatureType::Merfolk, CreatureType::Wizard],
        Color::Blue,
        mana_cost(1, &[(Color::Blue, 1)]),
        2,
        1,
        helpers::enters_prepared(VIBRANT_IDEA),
    );
    front.text = "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Vibrant Idea {4}{U} Sorcery — Draw two cards.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::cards::{build_game, grp};
    use crate::effects::ability::{Ability, EventPattern};
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[test]
    fn landscape_painter_prepares_and_vibrant_idea_draws_two() {
        let mut db = CardDb::default();
        register(&mut db);
        let f = db.get(LANDSCAPE_PAINTER).unwrap();
        assert!(matches!(f.abilities[0], Ability::Prepare { spell: VIBRANT_IDEA }));
        assert!(matches!(f.abilities[1], Ability::Triggered { event: EventPattern::SelfEnters, .. }));
        // Behaviour: the back face draws two (build_game's starter_db carries the registered card).
        let state = build_game(1, &[&[grp::ISLAND, grp::ISLAND, grp::FOREST], &[]]);
        let effect = state.card_db().get(VIBRANT_IDEA).unwrap().spell_effect().unwrap().clone();
        let hand0 = state.player(PlayerId(0)).hand.len();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), hand0 + 2, "Vibrant Idea drew two");
    }
}
