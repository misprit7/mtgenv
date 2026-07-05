//! Registered **emblem defs** — the reserved 9500+ `grp_id` block (see [`super::grp`]). An emblem
//! (CR 114) is an object with **no characteristics** other than the abilities of the def it points
//! at; [`Effect::CreateEmblem`](crate::effects::Effect::CreateEmblem) puts one into a player's
//! command zone. Each def carries [`Ability::FunctionsFrom`]`(vec![Zone::Command])` so `collect_
//! triggers`' command-zone scan fires its triggered abilities from there (CR 113.6 zone-of-function),
//! and no card_types/mana_cost (an emblem is not a real card — filtered from the deck-builder catalog).
//!
//! Card-agnostic law: an emblem's behaviour is *data* (this def's `Ability` list), never a name-match.

use crate::basics::Zone;
use crate::cards::{grp, CardDb, CardDef};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::PlayerFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::state::Characteristics;

pub fn register(db: &mut CardDb) {
    // Professor Dellian Fel's −6 emblem: "Whenever you gain life, target opponent loses that much
    // life." The `GainLife` trigger fires from the command zone (FunctionsFrom); the triggering
    // life-gain amount rides on the trigger's `x` (read as `ValueExpr::X` — "that much").
    db.insert(CardDef {
        chars: Characteristics {
            name: "Emblem — Dellian Fel".to_string(),
            grp_id: grp::DELLIAN_EMBLEM,
            // CR 114.2: an emblem has no characteristics (no card types, colours, cost, or P/T).
            ..Default::default()
        },
        abilities: vec![
            Ability::Triggered {
                event: EventPattern::GainLife,
                condition: None,
                intervening_if: false,
                effect: Effect::Sequence(vec![
                    Effect::TargetPlayer(PlayerFilter::Opponent),
                    Effect::LoseLife { who: PlayerRef::ChosenTarget(0), amount: ValueExpr::X },
                ]),
            },
            Ability::FunctionsFrom(vec![Zone::Command]),
        ],
        text: "Whenever you gain life, target opponent loses that much life.".to_string(),
        fully_implemented: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dellian_emblem_def_is_registered_and_functions_from_command() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::DELLIAN_EMBLEM).unwrap();
        assert!(def.chars.card_types.is_empty(), "an emblem has no characteristics (CR 114.2)");
        assert!(matches!(def.abilities[0], Ability::Triggered { event: EventPattern::GainLife, .. }));
        assert!(
            def.abilities.iter().any(|a| matches!(a, Ability::FunctionsFrom(z) if z.contains(&Zone::Command))),
            "its ability functions from the command zone",
        );
    }
}
