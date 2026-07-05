//! Tester of the Tangential — `{1}{U}` Creature — Djinn Wizard 1/1.
//!
//! Oracle: "Increment (Whenever you cast a spell, if the amount of mana you spent is greater than this
//! creature's power or toughness, put a +1/+1 counter on this creature.)
//! At the beginning of combat on your turn, you may pay {X}. When you do, move X +1/+1 counters from
//! this creature onto another target creature."
//!
//! **Tracked-partial** (`.incomplete()`): the **Increment** keyword is the shared, fully-faithful
//! `helpers::increment_ability()`. The combat ability is deferred: "you may pay {X}. When you do, move X
//! +1/+1 counters onto another target creature" needs three pieces not yet in the core — a `MayPayCost`
//! whose cost carries `{X}` (announcing X and threading it to the "when you do"), a **reflexive** target
//! chosen when that sub-trigger fires (CR 603.7c), and an `Effect::MoveCounters` (remove N from one
//! object, add N to another). Omitted rather than shipped as a no-op husk. See the sos-cards ledger's
//! counter-manipulation sketch.

use crate::basics::Color;
use crate::cards::helpers::increment_ability;
use crate::cards::{creature, mana_cost, CardDb};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const TESTER_OF_THE_TANGENTIAL: u32 = 438;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        TESTER_OF_THE_TANGENTIAL,
        "Tester of the Tangential",
        &[CreatureType::Djinn, CreatureType::Wizard],
        Color::Blue,
        mana_cost(1, &[(Color::Blue, 1)]),
        1,
        1,
        vec![increment_ability()],
    );
    def.text = "Increment (Whenever you cast a spell, if the amount of mana you spent is greater than this creature's power or toughness, put a +1/+1 counter on this creature.)\nAt the beginning of combat on your turn, you may pay {X}. When you do, move X +1/+1 counters from this creature onto another target creature.".to_string();
    db.insert(def.incomplete());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::ability::{Ability, EventPattern};

    #[test]
    fn tester_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TESTER_OF_THE_TANGENTIAL).unwrap();
        assert_eq!((def.chars.power, def.chars.toughness), (Some(1), Some(1)));
        assert_eq!(def.chars.colors, vec![Color::Blue]);
        assert!(!def.fully_implemented, "combat move-counters ability deferred");
        // The Increment keyword is present (a SpellCast trigger).
        assert!(matches!(
            def.abilities[0],
            Ability::Triggered { event: EventPattern::SpellCast(_), .. }
        ));
    }
}
