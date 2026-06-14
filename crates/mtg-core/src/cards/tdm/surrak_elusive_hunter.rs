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
//! - **"Whenever a creature you control becomes the target of a spell or ability an opponent
//!   controls, draw a card"** (the battlefield-creature half) — a `Triggered{ BecomesTargeted{
//!   filter: creature you control, by_opponent: true } }` → `Draw 1` (C16). Fires once per matching
//!   creature that becomes a target (CR 603.2e), so an opponent's spell hitting two of your creatures
//!   draws two.
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
//!   2. The **"creature SPELL you control"** half of the draw trigger — when an opponent targets your
//!      creature *spell on the stack* (not a permanent). The C16 event fires only for permanents
//!      (`Target::Object`); the stack-object case (`Target::Stack`) is a deferred capability. So the
//!      authored trigger is faithful for the common battlefield half and merely **under-triggers** the
//!      rarer stack half — an honest missing upside, never a wrong fire. Tracked until `Target::Stack`
//!      targeting exists.

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

/// "Whenever a creature you control … becomes the target of a spell or ability an opponent controls,
/// draw a card." (the battlefield-creature half — C16). Fires once per matching creature targeted by
/// an opponent-controlled source (CR 603.2e); the "creature spell you control" half awaits
/// `Target::Stack` targeting (tracked in the module docs).
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
    // subsystem), and the draw trigger covers only the battlefield-creature half — the "creature
    // spell you control" half awaits Target::Stack targeting. See module docs.
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
        // Tracked-incomplete: can't-be-countered is inert + the draw trigger covers only the
        // battlefield-creature half (stack-creature-spell half awaits Target::Stack).
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
