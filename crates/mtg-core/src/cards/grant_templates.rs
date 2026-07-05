//! Registered **granted-ability template defs** — the reserved 9800+ `grp_id` block (see
//! [`super::grp`]). Each def carries exactly ONE [`Ability::Triggered`] and represents an ability
//! that a continuous effect grants to a permanent (CR 613.1f — "gains '[ability]' until end of
//! turn"; [`crate::effects::Effect::GrantAbility`] /
//! [`crate::effects::ability::StaticContribution::GrantAbility`]).
//!
//! A `GrantAbility` continuous effect stores just the template's `grp_id` (serde-safe — an `Ability`
//! isn't); `queue_self_triggers` reads the template's trigger via `granted_ability_templates` and
//! fires it from the AFFECTED object (so "this creature" and the controller read correctly) with the
//! granting card's effect. These are not real cards (no card_types); the `/api/cards` catalog already
//! excludes everything ≥ 9700. Card-agnostic law: the granted behaviour is *data* (this def's ability).

use crate::cards::{grp, CardDb, CardDef};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::state::Characteristics;

/// Build a template def carrying a single triggered ability.
fn template(grp_id: u32, name: &str, event: EventPattern, effect: Effect, text: &str) -> CardDef {
    CardDef {
        chars: Characteristics { name: name.to_string(), grp_id, ..Default::default() },
        abilities: vec![Ability::Triggered { event, condition: None, intervening_if: false, effect }],
        text: text.to_string(),
        fully_implemented: true,
    }
}

pub fn register(db: &mut CardDb) {
    // "When this creature dies, draw a card." (Rabid Attack grants this.)
    db.insert(template(
        grp::GRANT_DIES_DRAW,
        "Granted — When this dies, draw a card",
        EventPattern::SelfDies,
        Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
        "When this creature dies, draw a card.",
    ));
    // "Whenever this creature attacks, you gain 1 life." (Root Manipulation grants this.)
    db.insert(template(
        grp::GRANT_ATTACKS_GAIN_LIFE,
        "Granted — Whenever this attacks, gain 1 life",
        EventPattern::SelfAttacks,
        Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
        "Whenever this creature attacks, you gain 1 life.",
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_registered_with_one_trigger_each() {
        let mut db = CardDb::default();
        register(&mut db);
        for grp_id in [grp::GRANT_DIES_DRAW, grp::GRANT_ATTACKS_GAIN_LIFE] {
            let def = db.get(grp_id).unwrap();
            assert!(def.chars.card_types.is_empty(), "not a real card (no card types)");
            assert_eq!(def.abilities.len(), 1, "exactly one ability");
            assert!(matches!(def.abilities[0], Ability::Triggered { .. }));
        }
        assert!(matches!(
            db.get(grp::GRANT_DIES_DRAW).unwrap().abilities[0],
            Ability::Triggered { event: EventPattern::SelfDies, .. }
        ));
    }
}
