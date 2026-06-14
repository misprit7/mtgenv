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
//! - **Warp {2}{G} — cast + exile** (C14 pieces 1+2, c445d78) — an `Ability::Warp { cost: {2}{G} }`:
//!   `legal_priority_actions` offers a sorcery-speed warp cast from hand (even when the normal
//!   {2}{G}{G} is unaffordable), `cast_spell` pays the warp cost, and on resolution the creature arms a
//!   `DelayedTriggerEvent::AtBeginningOfNextEndStep` trigger that exiles it (CR 702-warp). This half is
//!   **atomic and exploit-free** — the cheap cast always carries its exile downside, never a free discount.
//!
//! INCOMPLETE — TRACKED (`fully_implemented: false`), one minor gap, not approximated:
//!   - **Warp's recast-from-exile** — "then you may cast it from exile on a later turn." Needs the
//!     warp-exiled card to be marked castable-from-exile + `legal_priority_actions` to offer it from
//!     exile (C14 piece 3, pending). Until then a warp-cast Mightform is exiled and stays there — a
//!     faithful subset (missing upside), NOT a wrong approximation. Flips to fully-implemented when
//!     piece 3 lands, no other change.

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
            // "Warp {2}{G}" — alt-cast from hand for {2}{G}, then exile at the next end step (C14 1+2).
            Ability::Warp { cost: mana_cost(2, &[(Color::Green, 1)]) },
        ],
    );
    def.text = "Landfall — Whenever a land you control enters, double the power of target creature you control until end of turn.\nWarp {2}{G} (You may cast this card from your hand for its warp cost. Exile this creature at the beginning of the next end step, then you may cast it from exile on a later turn.)".to_string();
    // Tracked-incomplete: warp's cast + exile work (C14 1+2); only recast-from-exile (piece 3) is
    // pending. Flips to fully-implemented when piece 3 lands. See module docs.
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
        // Tracked-incomplete only on warp's recast-from-exile (piece 3); landfall double-power +
        // warp cast+exile (pieces 1+2) are implemented.
        assert!(!def.fully_implemented);
        // Landfall double-power pump (C15) + Warp {2}{G} alt-cast (C14 1+2).
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
                Warp {
                    cost: ManaCost {
                        generic: 2,
                        colored: {
                            Green: 1,
                        },
                        x: 0,
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }
}
