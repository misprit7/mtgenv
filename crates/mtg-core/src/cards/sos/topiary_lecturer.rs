//! Topiary Lecturer — `{2}{G}` Creature — Elf Druid 1/2 (first printed SOS).
//!
//! Oracle: "Increment (Whenever you cast a spell, if the amount of mana you spent is greater than
//! this creature's power or toughness, put a +1/+1 counter on this creature.) / {T}: Add an amount
//! of {G} equal to this creature's power."
//!
//! **Fully implemented** — the shared `increment_ability()` (a `SpellCast(Any)` trigger comparing
//! `ManaSpentOnTrigger` to the source's power/toughness) + a `{T}` mana ability adding `{G}` equal to
//! `ValueExpr::PowerOfSelf` (the same power-scaled mana Molten-Core Maestro produces). As Increment
//! grows its power, the mana ability scales with it.

use crate::basics::Color;
use crate::cards::helpers::increment_ability;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
use crate::effects::target::ManaSpec;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const TOPIARY_LECTURER: u32 = 338;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        TOPIARY_LECTURER,
        "Topiary Lecturer",
        &[CreatureType::Elf, CreatureType::Druid],
        Color::Green,
        mana_cost(2, &[(Color::Green, 1)]),
        1,
        2,
        vec![
            increment_ability(),
            Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::TapSelf] },
                effect: Effect::AddMana {
                    who: PlayerRef::Controller,
                    mana: ManaSpec {
                        produces: vec![(Color::Green, ValueExpr::PowerOfSelf)],
                        any_color: None,
                        one_of: None,
                        restriction: None,
                    },
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: true,
            },
        ],
    );
    def.text = "Increment (Whenever you cast a spell, if the amount of mana you spent is greater than this creature's power or toughness, put a +1/+1 counter on this creature.)\n{T}: Add an amount of {G} equal to this creature's power.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::{Color, CounterKind, Zone};
    use crate::cards::starter_db;
    use crate::effects::action::{ResolutionCtx, WbReason};
    use crate::ids::{PlayerId, StackId};
    use crate::priority::Engine;
    use crate::state::GameState;
    use std::sync::Arc;

    #[test]
    fn topiary_lecturer_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TOPIARY_LECTURER).unwrap();
        assert_eq!((def.chars.power, def.chars.toughness), (Some(1), Some(2)));
        assert!(def.fully_implemented);
        assert!(
            matches!(&def.abilities[1], Ability::Activated { is_mana: true, .. }),
            "the {{T}} ability is a mana ability"
        );
    }

    /// The `{T}` mana ability adds {G} equal to this creature's power — `PowerOfSelf`. At base power 1
    /// it yields one {G}; with a +1/+1 counter (power 2) it yields two. Resolves the AddMana effect
    /// directly (the mana-ability activation machinery is generic; the card-specific part is the
    /// power-scaled `ManaSpec`).
    fn green_from_power(counters: u32) -> u32 {
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let elf = {
            let c = state.card_db().get(TOPIARY_LECTURER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        if counters > 0 {
            state.objects.get_mut(&elf).unwrap().counters.counts.insert(CounterKind::PlusOnePlusOne, counters);
            state.mark_chars_dirty();
        }
        let effect = match &state.card_db().get(TOPIARY_LECTURER).unwrap().abilities[1] {
            Ability::Activated { effect, .. } => effect.clone(),
            o => panic!("{o:?}"),
        };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(elf), ..Default::default() },
            WbReason::Resolve(StackId(0)),
        );
        e.state.player(PlayerId(0)).mana_pool.amounts.get(&Color::Green).copied().unwrap_or(0)
    }

    #[test]
    fn taps_for_green_equal_to_power() {
        assert_eq!(green_from_power(0), 1, "base power 1 → one {{G}}");
        assert_eq!(green_from_power(2), 3, "with two +1/+1 counters (power 3) → three {{G}}");
    }
}
