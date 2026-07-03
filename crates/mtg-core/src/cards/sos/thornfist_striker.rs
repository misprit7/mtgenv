//! Thornfist Striker — `{2}{G}` Creature — Elf Druid 3/3 (first printed SOS).
//!
//! Oracle: "Ward {1} / Infusion — Creatures you control get +1/+0 and have trample as long as you
//! gained life this turn."
//!
//! **Fully implemented** — the fourth S17 Ward card (mana Ward), plus an **Infusion** conditional
//! team anthem: two `ConditionalStatic`s over "creatures you control" — a `ModifyPT{+1,0}` and a
//! `GrantKeyword(Trample)` — each gated on `Condition::GainedLifeThisTurn` (CR 611 continuous effect
//! that turns on/off with the life-gained state, the same primitive Comforting Counsel's anthem uses).

use crate::basics::Color;
use crate::cards::helpers::{creatures_you_control, ward_mana};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Keyword, StaticContribution};
use crate::effects::condition::{Condition, Duration};
use crate::effects::value::PlayerRef;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const THORNFIST_STRIKER: u32 = 339;

/// One half of Infusion's conditional team anthem: `contribution` applied to "creatures you control"
/// while you have gained life this turn.
fn infusion_static(contribution: StaticContribution) -> Ability {
    Ability::ConditionalStatic {
        contribution,
        affects: creatures_you_control(),
        duration: Duration::WhileSourcePresent,
        condition: Condition::GainedLifeThisTurn { who: PlayerRef::Controller },
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        THORNFIST_STRIKER,
        "Thornfist Striker",
        &[CreatureType::Elf, CreatureType::Druid],
        Color::Green,
        mana_cost(2, &[(Color::Green, 1)]),
        3,
        3,
        vec![
            ward_mana(1),
            infusion_static(StaticContribution::ModifyPT { power: 1, toughness: 0 }),
            infusion_static(StaticContribution::GrantKeyword(Keyword::Trample)),
        ],
    );
    def.text = "Ward {1} (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays {1}.)\nInfusion — Creatures you control get +1/+0 and have trample as long as you gained life this turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::Zone;
    use crate::cards::{grp, starter_db};
    use crate::ids::PlayerId;
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    #[test]
    fn thornfist_striker_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(THORNFIST_STRIKER).unwrap();
        assert_eq!((def.chars.power, def.chars.toughness), (Some(3), Some(3)));
        assert!(def.fully_implemented);
        assert!(
            matches!(&def.abilities[0], Ability::Triggered { .. }),
            "ability 0 is the Ward trigger"
        );
    }

    /// The Infusion anthem is live only while you gained life this turn: a bystander creature you
    /// control is 2/2 normally, 3/2 with trample once you've gained life.
    #[test]
    fn infusion_anthem_gated_on_gained_life() {
        use crate::effects::ability::Keyword;
        let build = |gained: bool| {
            let mut state = GameState::new(2, 1);
            state.set_card_db(Arc::new(starter_db()));
            {
                let c = state.card_db().get(THORNFIST_STRIKER).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield);
            }
            let bear = {
                let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(); // 2/2
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            if gained {
                state.players[0].life_gained_this_turn = 1;
            }
            use crate::agent::RandomAgent;
            let e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
            (e.state.computed(bear).power, e.state.computed(bear).has_keyword(Keyword::Trample))
        };
        assert_eq!(build(false), (Some(2), false), "no life gained → plain 2/2, no trample");
        assert_eq!(build(true), (Some(3), true), "gained life → +1/+0 and trample (3/2 trample)");
    }
}
