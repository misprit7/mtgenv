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
//! - **Warp {2}{G}** (C14, complete — c445d78 + 7cc6f9c) — an `Ability::Warp { cost: {2}{G} }`:
//!   `legal_priority_actions` offers a sorcery-speed warp cast from hand for {2}{G} (even when the
//!   normal {2}{G}{G} is unaffordable), `cast_spell` pays it, and on resolution the creature arms a
//!   `DelayedTriggerEvent::AtBeginningOfNextEndStep` trigger that exiles it via `Action::WarpExile`
//!   (a dedicated exile that grants recast permission — `Object.castable_from_exile` — so plain exiles
//!   don't). On a later turn it's offered for recast from exile at its normal {2}{G}{G} (sorcery speed)
//!   and resolves as a plain creature (no re-warp). CR 702-warp, end to end — atomic/exploit-free.
//!
//! **Fully implemented** — every printed clause faithful, no deferrals.

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
    // Fully implemented: landfall double-power (C15) + Warp {2}{G} end-to-end (C14, all 3 pieces).
    def.fully_implemented = true;
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
        // Fully implemented: landfall double-power (C15) + Warp {2}{G} end-to-end (C14, all 3 pieces).
        assert!(def.fully_implemented);
        // Landfall double-power pump (C15) + Warp {2}{G} alt-cast (C14).
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

    /// Behaviour: resolving the landfall pump on a 2/2 doubles its power to 4 (the `PowerOfTarget`
    /// snapshot), leaving toughness unchanged — the real C15 resolution, not just the IR.
    #[test]
    fn mightform_landfall_doubles_target_power() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears_chars = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(); // vanilla 2/2
        let mightform_chars = state.card_db().get(MIGHTFORM_HARMONIZER).unwrap().chars.clone();
        let bears = state.add_card(PlayerId(0), bears_chars, Zone::Battlefield);
        let mightform = state.add_card(PlayerId(0), mightform_chars, Zone::Battlefield);
        // The landfall ability's effect = the double-power pump.
        let pump = match &state.card_db().get(MIGHTFORM_HARMONIZER).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(),
            other => panic!("expected landfall Triggered, got {other:?}"),
        };
        let mut e = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
        );
        e.resolve_effect(
            &pump,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                source: Some(mightform),
                chosen_targets: vec![Target::Object(bears)],
                ..Default::default()
            },
            WbReason::Resolve(StackId(0)),
        );
        assert_eq!(e.state.computed(bears).power, Some(4)); // 2 + PowerOfTarget(2) snapshot = 4
        assert_eq!(e.state.computed(bears).toughness, Some(2)); // +0
    }
}
