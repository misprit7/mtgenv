//! Infirmary Healer // Stream of Life — `{1}{G}` Creature — Cat Cleric 2/3 // `{X}{G}` Sorcery
//! (first printed SOS). A **Prepare** DFC (enters prepared).
//!
//! Front: "This creature enters prepared."  Back (Stream of Life): "Target player gains X life."
//!
//! **Fully implemented** — enters-prepared via [`helpers::enters_prepared`]; the back is an `{X}` spell
//! (a target player gains X life). The prepared cast pays through `cast_spell`, which chooses `{X}`
//! and threads it onto the stack object so `ValueExpr::X` reads it at resolution.

use crate::basics::{CardType, Color, ManaCost};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::target::PlayerFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;
use std::collections::BTreeMap;

pub const INFIRMARY_HEALER: u32 = 396;
pub const STREAM_OF_LIFE: u32 = 9723;

pub fn register(db: &mut CardDb) {
    let stream_of_life = Effect::Sequence(vec![
        Effect::TargetPlayer(PlayerFilter::Any),
        Effect::GainLife { who: PlayerRef::ChosenTarget(0), amount: ValueExpr::X },
    ]);
    // {X}{G}: one green pip plus a variable `{X}`.
    let cost = ManaCost {
        generic: 0,
        colored: BTreeMap::from([(Color::Green, 1)]),
        x: 1,
        ..Default::default()
    };
    db.insert(
        spell(STREAM_OF_LIFE, "Stream of Life", CardType::Sorcery, Color::Green, cost, stream_of_life)
            .with_text("Target player gains X life."),
    );
    let mut front = creature(
        INFIRMARY_HEALER,
        "Infirmary Healer",
        &[CreatureType::Cat, CreatureType::Cleric],
        Color::Green,
        mana_cost(1, &[(Color::Green, 1)]),
        2,
        3,
        helpers::enters_prepared(STREAM_OF_LIFE),
    );
    front.text = "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Stream of Life {X}{G} Sorcery — Target player gains X life.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::Target;
    use crate::cards::build_game;
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;

    #[test]
    fn stream_of_life_gains_x_life() {
        let state = build_game(1, &[&[], &[]]);
        let effect = state.card_db().get(STREAM_OF_LIFE).unwrap().spell_effect().unwrap().clone();
        let life0 = state.player(PlayerId(0)).life;
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                x: Some(3),
                chosen_targets: vec![Target::Player(PlayerId(0))],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).life, life0 + 3, "gained X=3 life");
    }
}
