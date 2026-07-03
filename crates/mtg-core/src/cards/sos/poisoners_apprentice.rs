//! Poisoner's Apprentice — `{2}{B}` Creature — Orc Warlock 2/2 (first printed SOS).
//!
//! Oracle: "Infusion — When this creature enters, target creature an opponent controls gets -4/-4
//! until end of turn if you gained life this turn."
//!
//! **Fully implemented** — an ETB `Conditional` on the Infusion gate: if you gained life this turn,
//! a target creature an opponent controls gets -4/-4 until end of turn. Because the conditional's
//! body targets, the real ETB defers target choice to a reflexive sub-trigger (CR 603.7c) — the
//! target is only chosen when the condition is met (the established Earthbender Ascension pattern).

use crate::basics::Color;
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::condition::{Condition, Duration};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const POISONERS_APPRENTICE: u32 = 253;

pub fn register(db: &mut CardDb) {
    db.insert(
        creature(
            POISONERS_APPRENTICE,
            "Poisoner's Apprentice",
            &[CreatureType::Orc, CreatureType::Warlock],
            Color::Black,
            mana_cost(2, &[(Color::Black, 1)]),
            2,
            2,
            vec![Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::Conditional {
                    cond: Condition::GainedLifeThisTurn { who: PlayerRef::Controller },
                    then: Box::new(Effect::PumpPT {
                        what: EffectTarget::Target(TargetSpec {
                            kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Opponent)),
                            min: 1,
                            max: 1,
                            distinct: true,
                        }),
                        power: ValueExpr::Fixed(-4),
                        toughness: ValueExpr::Fixed(-4),
                        duration: Duration::UntilEndOfTurn,
                    }),
                    otherwise: None,
                },
            }],
        )
        .with_text("Infusion — When this creature enters, target creature an opponent controls gets -4/-4 until end of turn if you gained life this turn."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn poisoners_apprentice_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(POISONERS_APPRENTICE).unwrap();
        assert_eq!(def.chars.power, Some(2));
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: Conditional {
                        cond: GainedLifeThisTurn {
                            who: Controller,
                        },
                        then: PumpPT {
                            what: Target(
                                TargetSpec {
                                    kind: Creature(
                                        ControlledBy(
                                            Opponent,
                                        ),
                                    ),
                                    min: 1,
                                    max: 1,
                                    distinct: true,
                                },
                            ),
                            power: Fixed(
                                -4,
                            ),
                            toughness: Fixed(
                                -4,
                            ),
                            duration: UntilEndOfTurn,
                        },
                        otherwise: None,
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// Behaviour: the Infusion debuff applies -4/-4 only when life was gained this turn. (Resolved
    /// inline here — no `ability_index` in the ctx — so the effect logic is exercised directly; the
    /// reflexive-deferral path is validated by Earthbender Ascension.)
    #[test]
    fn poisoners_apprentice_debuffs_on_infusion() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;

        let toughness_after = |life_gained: u32| {
            let mut state = build_game(1, &[&[], &[]]);
            state.players[0].life_gained_this_turn = life_gained;
            let src = {
                let c = state.card_db().get(POISONERS_APPRENTICE).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            let victim = {
                let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
                state.add_card(PlayerId(1), c, Zone::Battlefield)
            };
            let etb = match &state.card_db().get(POISONERS_APPRENTICE).unwrap().abilities[0] {
                Ability::Triggered { effect, .. } => effect.clone(),
                o => panic!("expected ETB Triggered, got {o:?}"),
            };
            let mut e = Engine::new(
                state,
                vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))],
            );
            e.resolve_effect(
                &etb,
                &ResolutionCtx {
                    controller: Some(PlayerId(0)),
                    source: Some(src),
                    chosen_targets: vec![Target::Object(victim)],
                    ..Default::default()
                },
                WbReason::Resolve(StackId(0)),
            );
            e.state.computed(victim).toughness
        };
        assert_eq!(toughness_after(1), Some(-2), "gained life → -4/-4 on the 2/2 → -2 toughness");
        assert_eq!(toughness_after(0), Some(2), "no life gained → untouched 2/2");
    }
}
