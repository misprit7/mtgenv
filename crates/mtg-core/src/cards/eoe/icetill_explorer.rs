//! Icetill Explorer — `{2}{G}{G}` Creature — Insect Scout 2/4 (first printed EOE, Edge of Eternities).
//!
//! Oracle:
//!   You may play an additional land on each of your turns.
//!   You may play lands from your graveyard.
//!   Landfall — Whenever a land you control enters, mill a card.
//!
//! IMPLEMENTED: the landfall mill (C4 land-you-control trigger → C3 `Mill`).
//!
//! INCOMPLETE — TRACKED (needs the unbuilt **land-play-permission** subsystem, C18):
//!   • "play an additional land on each of your turns" (CR 305.2 / 505 extra land drops);
//!   • "play lands from your graveyard" (playing a land from a non-hand zone).
//! These are continuous permission-granting statics with no engine layer yet (flagged to
//! engine/lead). Per the fidelity standard this card is registered with ONLY its faithful
//! landfall ability — it is deliberately *missing* these clauses, not shipped with a wrong
//! approximation of them. Add the two statics here once the C18 subsystem exists.

use crate::basics::Color;
use crate::cards::helpers::land_you_control;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (per-set ids live near their cards).
pub const ICETILL_EXPLORER: u32 = 104;

pub fn register(db: &mut CardDb) {
    let mut explorer = creature(
        ICETILL_EXPLORER,
        "Icetill Explorer",
        "Insect",
        Color::Green,
        mana_cost(2, &[(Color::Green, 2)]),
        2,
        4,
        vec![Ability::Triggered {
            event: EventPattern::PermanentEnters(land_you_control()),
            condition: None,
            intervening_if: false,
            effect: Effect::Mill {
                who: PlayerRef::Controller,
                count: ValueExpr::Fixed(1),
            },
        }],
    );
    explorer.chars.subtypes = vec!["Insect".to_string(), "Scout".to_string()];
    db.insert(explorer.with_text(
        "You may play an additional land on each of your turns.\nYou may play lands from your graveyard.\nLandfall — Whenever a land you control enters, mill a card.",
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn icetill_explorer_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ICETILL_EXPLORER).unwrap();
        assert_eq!(def.chars.power, Some(2));
        assert_eq!(def.chars.toughness, Some(4));
        assert_eq!(def.chars.subtypes, vec!["Insect".to_string(), "Scout".to_string()]);
        assert!(!def.is_mana_source());
        // Only the faithful landfall-mill ability is present (land-play permissions are TRACKED
        // incomplete, C18 — see module docs).
        expect![[r#"
            [
                Triggered {
                    event: PermanentEnters(
                        All(
                            [
                                HasCardType(
                                    Land,
                                ),
                                ControlledBy(
                                    Controller,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: Mill {
                        who: Controller,
                        count: Fixed(
                            1,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
