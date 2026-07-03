//! Ulna Alley Shopkeep — `{2}{B}` Creature — Goblin Warlock 2/3 (first printed SOS).
//!
//! Oracle: "Menace / Infusion — This creature gets +2/+0 as long as you gained life this turn."
//!
//! **Fully implemented** — printed Menace plus a `ConditionalStatic` that adds +2/+0 to itself while
//! the Infusion condition holds (`GainedLifeThisTurn`), toggling on/off as life is gained.

use crate::basics::Color;
use crate::cards::helpers::itself;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Keyword, StaticContribution};
use crate::effects::condition::{Condition, Duration};
use crate::effects::value::PlayerRef;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ULNA_ALLEY_SHOPKEEP: u32 = 238;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        ULNA_ALLEY_SHOPKEEP,
        "Ulna Alley Shopkeep",
        &[CreatureType::Goblin, CreatureType::Warlock],
        Color::Black,
        mana_cost(2, &[(Color::Black, 1)]),
        2,
        3,
        vec![Ability::ConditionalStatic {
            contribution: StaticContribution::ModifyPT { power: 2, toughness: 0 },
            affects: itself(),
            duration: Duration::WhileSourcePresent,
            condition: Condition::GainedLifeThisTurn { who: PlayerRef::Controller },
        }],
    );
    def.chars.keywords = vec![Keyword::Menace];
    def.text = "Menace\nInfusion — This creature gets +2/+0 as long as you gained life this turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulna_alley_shopkeep_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ULNA_ALLEY_SHOPKEEP).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Menace]);
        assert!(def.fully_implemented);
    }

    /// Behaviour: +2/+0 applies only while life was gained this turn (the ConditionalStatic toggles).
    #[test]
    fn ulna_alley_shopkeep_infusion_toggles_power() {
        use crate::agent::RandomAgent;
        use crate::basics::Zone;
        use crate::cards::build_game;
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let chars = state.card_db().get(ULNA_ALLEY_SHOPKEEP).unwrap().chars.clone();
        let src = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        assert_eq!(e.state.computed(src).power, Some(2), "no life gained → base 2/3");
        e.state.players[0].life_gained_this_turn = 1;
        e.state.mark_chars_dirty();
        assert_eq!(e.state.computed(src).power, Some(4), "gained life → +2/+0 → 4/3");
    }
}
