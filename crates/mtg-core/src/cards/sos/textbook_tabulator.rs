//! Textbook Tabulator — `{2}{U}` Creature — Frog Wizard 0/3 (first printed SOS).
//!
//! Oracle: "Increment (…) / When this creature enters, surveil 2."
//!
//! **Fully implemented** — the shared Increment cast-trigger plus an ETB `Surveil 2`.

use crate::basics::Color;
use crate::cards::helpers::increment_ability;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::ValueExpr;
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const TEXTBOOK_TABULATOR: u32 = 266;

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            TEXTBOOK_TABULATOR,
            "Textbook Tabulator",
            &[CreatureType::Frog, CreatureType::Wizard],
            Color::Blue,
            mana_cost(2, &[(Color::Blue, 1)]),
            0,
            3,
            vec![
                increment_ability(),
                Ability::Triggered {
                    event: EventPattern::SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Effect::Surveil { count: ValueExpr::Fixed(2) },
                },
            ],
        )
        .with_text("Increment (Whenever you cast a spell, if the amount of mana you spent is greater than this creature's power or toughness, put a +1/+1 counter on this creature.)\nWhen this creature enters, surveil 2."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textbook_tabulator_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TEXTBOOK_TABULATOR).unwrap();
        assert_eq!(def.chars.power, Some(0));
        assert_eq!(def.chars.toughness, Some(3));
        assert_eq!(def.abilities.len(), 2, "Increment + ETB surveil");
        assert!(def.fully_implemented);
    }
}
