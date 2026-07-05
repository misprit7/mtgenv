//! Campus Composer // Aqueous Aria — `{3}{U}` Creature — Merfolk Bard 3/4 // `{4}{U}` Sorcery
//! (first printed SOS). A **Prepare** DFC (enters prepared).
//!
//! Front: "This creature enters prepared."  Back (Aqueous Aria): "Create a 3/3 blue and red Elemental
//! creature token with flying."
//!
//! **Fully implemented** — enters-prepared via [`helpers::enters_prepared`]; the back creates the
//! shared [`helpers::elemental_token`] (3/3 U/R flier).

use crate::basics::{CardType, Color};
use crate::cards::{creature, helpers, mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

pub const CAMPUS_COMPOSER: u32 = 383;
pub const AQUEOUS_ARIA: u32 = 9710;

pub fn register(db: &mut CardDb) {
    db.insert(
        spell(
            AQUEOUS_ARIA,
            "Aqueous Aria",
            CardType::Sorcery,
            Color::Blue,
            mana_cost(4, &[(Color::Blue, 1)]),
            Effect::CreateToken {
                spec: helpers::elemental_token(),
                count: ValueExpr::Fixed(1),
                controller: PlayerRef::Controller,
                dynamic_counters: Vec::new(),
            },
        )
        .with_text("Create a 3/3 blue and red Elemental creature token with flying."),
    );
    let mut front = creature(
        CAMPUS_COMPOSER,
        "Campus Composer",
        &[CreatureType::Merfolk, CreatureType::Bard],
        Color::Blue,
        mana_cost(3, &[(Color::Blue, 1)]),
        3,
        4,
        helpers::enters_prepared(AQUEOUS_ARIA),
    );
    front.text = "This creature enters prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)\n// Aqueous Aria {4}{U} Sorcery — Create a 3/3 blue and red Elemental creature token with flying.".to_string();
    db.insert(front);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::cards::build_game;
    use crate::effects::ability::Keyword;
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use crate::subtypes::{CreatureType, Subtype};

    #[test]
    fn aqueous_aria_makes_a_flying_elemental() {
        let state = build_game(1, &[&[], &[]]);
        let effect = state.card_db().get(AQUEOUS_ARIA).unwrap().spell_effect().unwrap().clone();
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        let elemental = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .find(|&&o| e.state.object(o).chars.subtypes.contains(&Subtype::Creature(CreatureType::Elemental)))
            .copied();
        let elemental = elemental.expect("an Elemental token was created");
        assert_eq!(e.state.object(elemental).chars.power, Some(3), "3/3 Elemental");
        assert!(e.state.computed(elemental).has_keyword(Keyword::Flying), "with flying");
    }
}
