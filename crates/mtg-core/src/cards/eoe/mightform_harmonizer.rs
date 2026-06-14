//! Mightform Harmonizer — `{2}{G}{G}` Creature — Insect Druid 4/4 (first printed EOE, Edge of
//! Eternities).
//!
//! Oracle:
//!   Landfall — Whenever a land you control enters, double the power of target creature you control
//!   until end of turn.
//!   Warp {2}{G} (You may cast this card from your hand for its warp cost. Exile this creature at the
//!   beginning of the next end step, then you may cast it from exile on a later turn.)
//!
//! IMPLEMENTED:
//! - **Landfall — "double the power of target creature you control until end of turn"** — a
//!   `Triggered{PermanentEnters(land you control)}` (the shared landfall event) over
//!   `Effect::PumpPT{ what: target creature you control, power: PowerOfTarget(0), toughness: 0,
//!   UntilEndOfTurn }` (C15). "Double power" is the CR-correct one-shot **snapshot**: at resolution
//!   it grants +X/+0 where X = the target's power *at that moment* (`PowerOfTarget` reads the computed
//!   power once), fixed for the turn — it does NOT recompute if the creature's power later changes.
//!   The pump wears off at cleanup (CR 514.2).
//!
//! INCOMPLETE — TRACKED (`fully_implemented: false`), one gap, not approximated:
//!   - **Warp {2}{G}** — the alternative-cost casting mechanic (cast from hand for the warp cost, exile
//!     at the next end step, then cast from exile on a later turn). A genuine casting-permission
//!     subsystem (alt cost + delayed exile + play-from-exile), **C14**, still unbuilt. The card can be
//!     cast normally for {2}{G}{G}; only the warp discount/recast path is missing. Flagged to engine.

use crate::basics::Color;
use crate::cards::helpers::land_you_control;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const MIGHTFORM_HARMONIZER: u32 = 115;

pub fn register(db: &mut CardDb) {
    // "double the power of target creature you control until end of turn" — +X/+0 (X = its power at
    // resolution) for the turn. PowerOfTarget(0) snapshots the 0th chosen target's computed power.
    let double_power = Effect::PumpPT {
        what: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
            min: 1,
            max: 1,
            distinct: true,
        }),
        power: ValueExpr::PowerOfTarget(0),
        toughness: ValueExpr::Fixed(0),
        duration: Duration::UntilEndOfTurn,
    };
    let mut def = creature(
        MIGHTFORM_HARMONIZER,
        "Mightform Harmonizer",
        &[CreatureType::Insect, CreatureType::Druid],
        Color::Green,
        mana_cost(2, &[(Color::Green, 2)]),
        4,
        4,
        vec![
            // "Landfall — Whenever a land you control enters, double the power of target creature you
            // control until end of turn."
            Ability::Triggered {
                event: EventPattern::PermanentEnters(land_you_control()),
                condition: None,
                intervening_if: false,
                effect: double_power,
            },
        ],
    );
    def.text = "Landfall — Whenever a land you control enters, double the power of target creature you control until end of turn.\nWarp {2}{G} (You may cast this card from your hand for its warp cost. Exile this creature at the beginning of the next end step, then you may cast it from exile on a later turn.)".to_string();
    // Tracked-incomplete: the warp alternative-cost casting mechanic (C14) is unbuilt. Casting for
    // the full {2}{G}{G} works; only the warp discount/recast path is missing. See module docs.
    def.fully_implemented = false;
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;
    use crate::subtypes::Subtype;
    use expect_test::expect;

    #[test]
    fn mightform_harmonizer_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(MIGHTFORM_HARMONIZER).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Creature]);
        assert_eq!(
            def.chars.subtypes,
            vec![Subtype::Creature(CreatureType::Insect), Subtype::Creature(CreatureType::Druid)]
        );
        assert_eq!((def.chars.power, def.chars.toughness), (Some(4), Some(4)));
        // Tracked-incomplete: warp (C14) unbuilt; the landfall double-power is fully implemented.
        assert!(!def.fully_implemented);
        // Landfall trigger → snapshot double-power pump on target creature you control.
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
                    effect: PumpPT {
                        what: Target(
                            TargetSpec {
                                kind: Creature(
                                    ControlledBy(
                                        Controller,
                                    ),
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        power: PowerOfTarget(
                            0,
                        ),
                        toughness: Fixed(
                            0,
                        ),
                        duration: UntilEndOfTurn,
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }
}
