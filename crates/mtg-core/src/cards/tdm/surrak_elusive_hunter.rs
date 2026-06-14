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
//! - **"Whenever a creature you control or a creature spell you control becomes the target of a spell
//!   or ability an opponent controls, draw a card"** — a `Triggered{ BecomesTargeted{ filter: creature
//!   you control, by_opponent: true } }` → `Draw 1`. Covers **both halves**: the battlefield-creature
//!   half (C16, `8d006fd`) and the **creature-spell-on-stack half** (cap `d3ee9e9`) — the engine now
//!   fires `BecomesTargeted` for stack objects too, and the same filter (`HasCardType(Creature) +
//!   ControlledBy(Controller)`) matches a creature *spell* on the stack, so no IR change was needed.
//!   Fires once per matching object that becomes a target (CR 603.2e).
//!
//! INCOMPLETE — TRACKED (`fully_implemented: false`), one remaining capability gap, NOT approximated:
//!   - **"This spell can't be countered."** Modeled faithfully as the CR-correct static ability
//!     that functions while the spell is on the stack (CR 113.6f / 604.5): a `Qualification(
//!     CantBeCountered)` painted on `ItSelf` in `Zone::Stack`. It is **inert today** on two counts —
//!     (a) `chars::gather_statics` only walks the battlefield, so a stack-zone static is never
//!     gathered; (b) nothing in the engine reads `CantBeCountered` and **there is no counterspell in
//!     the card pool**, so the qualification has nothing to act on. Per the lead, this is deferred (a
//!     documented standing gap) until a counter subsystem exists — building it now is pure
//!     infrastructure for zero current effect. The IR declares the intent; the engine grows to interpret it.

use crate::basics::{CardType, Color, Zone};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, Keyword, Qualification, StaticContribution};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
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

/// "Whenever a creature you control or a creature spell you control becomes the target of a spell or
/// ability an opponent controls, draw a card." Fires once per matching object (CR 603.2e) — the same
/// filter covers both the battlefield-creature half (C16) and the creature-spell-on-stack half (d3ee9e9).
fn becomes_targeted_draw() -> Ability {
    Ability::Triggered {
        event: EventPattern::BecomesTargeted {
            filter: CardFilter::All(vec![
                CardFilter::HasCardType(CardType::Creature),
                CardFilter::ControlledBy(PlayerRef::Controller),
            ]),
            by_opponent: true,
        },
        condition: None,
        intervening_if: false,
        effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
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
        vec![cant_be_countered(), becomes_targeted_draw()],
    );
    def.chars.supertypes = vec![Supertype::Legendary];
    def.chars.keywords = vec![Keyword::Trample];
    def.text = "This spell can't be countered.\nTrample\nWhenever a creature you control or a creature spell you control becomes the target of a spell or ability an opponent controls, draw a card.".to_string();
    // Tracked-incomplete: "can't be countered" is inert (no stack-static gathering / no counter
    // subsystem); the draw trigger now covers BOTH halves (battlefield creature + creature spell on
    // the stack, cap d3ee9e9). Only can't-be-countered remains deferred. See module docs.
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
        // Tracked-incomplete on ONLY the inert can't-be-countered (no counterspell in the pool); the
        // draw trigger now covers both the battlefield-creature and creature-spell-on-stack halves.
        assert!(!def.fully_implemented);
        // The can't-be-countered static + the becomes-targeted draw trigger (battlefield half, C16).
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
                Triggered {
                    event: BecomesTargeted {
                        filter: All(
                            [
                                HasCardType(
                                    Creature,
                                ),
                                ControlledBy(
                                    Controller,
                                ),
                            ],
                        ),
                        by_opponent: true,
                    },
                    condition: None,
                    intervening_if: false,
                    effect: Draw {
                        who: Controller,
                        count: Fixed(
                            1,
                        ),
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }
}
