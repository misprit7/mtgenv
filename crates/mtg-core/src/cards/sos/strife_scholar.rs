//! Strife Scholar // Awaken the Ages — `{2}{R}` Creature — Orc Sorcerer 3/2 // `{5}{R}` Sorcery
//! (first printed SOS). A **Prepare** DFC (enters prepared).
//!
//! Front: "This creature enters prepared."  Back (Awaken the Ages): "Create two 2/2 red and white
//! Spirit creature tokens."
//!
//! **Fully implemented** — enters-prepared via [`helpers::enters_prepared`]; the back creates two of
//! the shared [`helpers::spirit_token`].

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

pub const STRIFE_SCHOLAR: u32 = 382;
pub const AWAKEN_THE_AGES: u32 = 9709;

pub fn register(db: &mut CardDb) {
    db.insert(
        spell(
            AWAKEN_THE_AGES,
            "Awaken the Ages",
            CardType::Sorcery,
            Color::Red,
            mana_cost(5, &[(Color::Red, 1)]),
            Effect::CreateToken {
                spec: helpers::spirit_token(),
                count: ValueExpr::Fixed(2),
                controller: PlayerRef::Controller,
                dynamic_counters: Vec::new(),
            },
        )
        .with_text("Create two 2/2 red and white Spirit creature tokens."),
    );
    let mut front = creature(
        STRIFE_SCHOLAR,
        "Strife Scholar",
        &[CreatureType::Orc, CreatureType::Sorcerer],
        Color::Red,
        mana_cost(2, &[(Color::Red, 1)]),
        3,
        2,
        helpers::enters_prepared(AWAKEN_THE_AGES),
    );
    front.text = "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Awaken the Ages {5}{R} Sorcery — Create two 2/2 red and white Spirit creature tokens.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::cards::build_game;
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use crate::subtypes::{CreatureType, Subtype};

    #[test]
    fn awaken_the_ages_makes_two_spirits() {
        let state = build_game(1, &[&[], &[]]);
        let effect = state.card_db().get(AWAKEN_THE_AGES).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let spirits = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .filter(|&&o| e.state.object(o).chars.subtypes.contains(&Subtype::Creature(CreatureType::Spirit)))
            .count();
        assert_eq!(spirits, 2, "Awaken the Ages made two Spirit tokens");
    }
}
