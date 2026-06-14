//! Surrak, Elusive Hunter — `{2}{G}` Legendary Creature — Human Warrior 4/3 (first printed TDM,
//! Tarkir: Dragonstorm; `tdm` is the expansion, `ptdm` is the promo printing).
//!
//! Oracle:
//!   This spell can't be countered.
//!   Trample
//!   Whenever a creature you control or a creature spell you control becomes the target of a spell
//!   or ability an opponent controls, draw a card.
//!
//! IMPLEMENTED:
//! - **Trample** (CR 702.19) — a printed `Keyword`, read by combat-damage assignment today.
//! - 4/3 P/T, Legendary supertype, Human Warrior subtypes (printed characteristics).
//!
//! INCOMPLETE — TRACKED (`fully_implemented: false`), two genuine capability gaps, NOT approximated:
//!   1. **"This spell can't be countered."** Modeled faithfully as the CR-correct static ability
//!      that functions while the spell is on the stack (CR 113.6f / 604.5): a `Qualification(
//!      CantBeCountered)` painted on `ItSelf` in `Zone::Stack`. It is **inert today** on two counts —
//!      (a) `chars::gather_statics` only walks the battlefield, so a stack-zone static is never
//!      gathered; (b) nothing in the engine reads `CantBeCountered` and there is no counter
//!      subsystem in the pool. The IR declares the intent; the engine grows to interpret it. Flagged
//!      to engine (capability: stack-zone static gathering + a counter check that respects the
//!      qualification).
//!   2. **"Whenever a creature you control / a creature spell you control becomes the target of a
//!      spell or ability an opponent controls, draw a card."** Needs the **becomes-targeted** event
//!      pattern (engine cap **C16**) — a trigger keyed on an object/stack-object *becoming a target*,
//!      scoped to opponent-controlled sources. Left unbuilt until C16 lands; authoring the trigger
//!      then is a one-liner against the existing `Effect::Draw`. Not approximated.

use crate::basics::{Color, Zone};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, Keyword, Qualification, StaticContribution};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const SURRAK_ELUSIVE_HUNTER: u32 = 112;

/// "This spell can't be countered." — a static ability that functions while the spell is on the
/// stack (CR 604.5 / 113.6f), painting the `CantBeCountered` qualification on the spell itself.
/// Inert until the engine gathers stack-zone statics and a counter check reads the marker (tracked).
fn cant_be_countered() -> Ability {
    Ability::Static {
        contribution: StaticContribution::Qualification(Qualification::CantBeCountered),
        affects: SelectSpec {
            zone: Zone::Stack,
            filter: CardFilter::ItSelf,
            chooser: PlayerRef::Controller,
            // min/max are unused for statics (the marker applies to every match — here, itself).
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(0),
        },
        duration: Duration::WhileSourcePresent,
    }
}

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        SURRAK_ELUSIVE_HUNTER,
        "Surrak, Elusive Hunter",
        &[CreatureType::Human, CreatureType::Warrior],
        Color::Green,
        mana_cost(2, &[(Color::Green, 1)]),
        4,
        3,
        vec![cant_be_countered()],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.chars.keywords = vec![Keyword::Trample];
    def.text = "This spell can't be countered.\nTrample\nWhenever a creature you control or a creature spell you control becomes the target of a spell or ability an opponent controls, draw a card.".to_string();
    // Tracked-incomplete: "can't be countered" is inert (no stack-static gathering / no counter
    // subsystem), and the becomes-targeted draw trigger needs engine cap C16. See module docs.
    def.fully_implemented = false;
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;
    use expect_test::expect;

    #[test]
    fn surrak_elusive_hunter_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SURRAK_ELUSIVE_HUNTER).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!(def.chars.keywords, vec![Keyword::Trample]); // trample works today
        assert_eq!(def.chars.power, Some(4));
        assert_eq!(def.chars.toughness, Some(3));
        // Tracked-incomplete: can't-be-countered is inert + becomes-targeted trigger needs C16.
        assert!(!def.fully_implemented);
        // Only the can't-be-countered static is materialized; the becomes-targeted trigger is
        // deliberately absent until C16 (no silent approximation).
        expect![[r#"
            [
                Static {
                    contribution: Qualification(
                        CantBeCountered,
                    ),
                    affects: SelectSpec {
                        zone: Stack,
                        filter: ItSelf,
                        chooser: Controller,
                        min: Fixed(
                            0,
                        ),
                        max: Fixed(
                            0,
                        ),
                    },
                    duration: WhileSourcePresent,
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }
}
