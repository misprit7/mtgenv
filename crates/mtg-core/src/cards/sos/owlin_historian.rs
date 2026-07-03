//! Owlin Historian — `{2}{W}` Creature — Bird Cleric 2/3 (first printed SOS).
//!
//! Oracle: "Flying / When this creature enters, surveil 1. / Whenever one or more cards leave your
//! graveyard, this creature gets +1/+1 until end of turn."
//!
//! **Fully implemented** — printed Flying + an ETB Surveil 1 + a `CardsLeaveYourGraveyard` self-pump.

use crate::basics::Color;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword};
use crate::effects::condition::Duration;
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const OWLIN_HISTORIAN: u32 = 303;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        OWLIN_HISTORIAN,
        "Owlin Historian",
        &[CreatureType::Bird, CreatureType::Cleric],
        Color::White,
        mana_cost(2, &[(Color::White, 1)]),
        2,
        3,
        vec![
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::Surveil { count: ValueExpr::Fixed(1) },
            },
            Ability::Triggered {
                event: EventPattern::CardsLeaveYourGraveyard,
                condition: None,
                intervening_if: false,
                effect: Effect::PumpPT {
                    what: EffectTarget::SourceSelf,
                    power: ValueExpr::Fixed(1),
                    toughness: ValueExpr::Fixed(1),
                    duration: Duration::UntilEndOfTurn,
                },
            },
        ],
    );
    def.chars.keywords = vec![Keyword::Flying];
    def.text = "Flying\nWhen this creature enters, surveil 1.\nWhenever one or more cards leave your graveyard, this creature gets +1/+1 until end of turn.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owlin_historian_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(OWLIN_HISTORIAN).unwrap();
        assert_eq!(def.chars.keywords, vec![Keyword::Flying]);
        assert_eq!(def.abilities.len(), 2, "ETB surveil + gy-leave pump");
        assert!(def.fully_implemented);
    }
}
