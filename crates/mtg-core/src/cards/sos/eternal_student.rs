//! Eternal Student — `{3}{B}` Creature — Zombie Warlock 4/2 (first printed SOS).
//!
//! Oracle: "{1}{B}, Exile this card from your graveyard: Create two 1/1 white and black Inkling
//! creature tokens with flying."
//!
//! **Fully implemented** — a vanilla 4/2 with a **graveyard-activated** ability (`{1}{B}` + exile this
//! from the graveyard → two Inkling tokens). Exercises the S18 graveyard-activation path.

use crate::basics::Color;
use crate::cards::helpers::inkling_token;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ETERNAL_STUDENT: u32 = 297;

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            ETERNAL_STUDENT,
            "Eternal Student",
            &[CreatureType::Zombie, CreatureType::Warlock],
            Color::Black,
            mana_cost(3, &[(Color::Black, 1)]),
            4,
            2,
            vec![Ability::Activated {
                cost: Cost {
                    mana: Some(mana_cost(1, &[(Color::Black, 1)])),
                    components: vec![CostComponent::ExileSelfFromGraveyard],
                },
                effect: Effect::CreateToken {
                    spec: inkling_token(),
                    count: ValueExpr::Fixed(2),
                    controller: PlayerRef::Controller,
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            }],
        )
        .with_text("{1}{B}, Exile this card from your graveyard: Create two 1/1 white and black Inkling creature tokens with flying."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eternal_student_is_graveyard_activated() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ETERNAL_STUDENT).unwrap();
        assert!(def.fully_implemented);
        let gy_act = matches!(&def.abilities[0], Ability::Activated { cost, .. }
            if cost.components.iter().any(|c| matches!(c, CostComponent::ExileSelfFromGraveyard)));
        assert!(gy_act, "carries the graveyard-activation cost component");
    }
}
