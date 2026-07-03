//! Graduation Day — `{W}` Enchantment (first printed SOS).
//!
//! Oracle: "Repartee — Whenever you cast an instant or sorcery spell that targets a creature, put a
//! +1/+1 counter on target creature you control."
//!
//! **Fully implemented** — a Repartee cast-trigger putting a +1/+1 counter on a target creature you
//! control.

use crate::basics::{Color, CounterKind};
use crate::cards::helpers::instant_or_sorcery;
use crate::cards::{enchantment, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};

/// grp id (per-set ids live near their cards).
pub const GRADUATION_DAY: u32 = 267;

pub fn register(db: &mut CardDb) {
    db.insert(
        enchantment(
            GRADUATION_DAY,
            "Graduation Day",
            Color::White,
            mana_cost(0, &[(Color::White, 1)]),
            vec![Ability::Triggered {
                event: EventPattern::SpellCastTargetingCreature(instant_or_sorcery()),
                condition: None,
                intervening_if: false,
                effect: Effect::PutCounters {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(1),
                },
            }],
        )
        .with_text("Repartee — Whenever you cast an instant or sorcery spell that targets a creature, put a +1/+1 counter on target creature you control."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn graduation_day_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        assert!(db.get(GRADUATION_DAY).unwrap().fully_implemented);
        expect![[r#"
            [
                Triggered {
                    event: SpellCastTargetingCreature(
                        AnyOf(
                            [
                                HasCardType(
                                    Instant,
                                ),
                                HasCardType(
                                    Sorcery,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: PutCounters {
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
                        kind: PlusOnePlusOne,
                        n: Fixed(
                            1,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", db.get(GRADUATION_DAY).unwrap().abilities));
    }

    #[test]
    fn graduation_day_repartee_counters_target() {
        use crate::agent::RandomAgent;
        use crate::basics::{Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::ids::{PlayerId, StackId};
        use crate::priority::Engine;
        let mut state = build_game(1, &[&[], &[]]);
        let bears = state.add_card(PlayerId(0), state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone(), Zone::Battlefield);
        let eff = match &state.card_db().get(GRADUATION_DAY).unwrap().abilities[0] {
            Ability::Triggered { effect, .. } => effect.clone(), o => panic!("{o:?}") };
        let mut e = Engine::new(state, vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(1))]);
        e.resolve_effect(&eff, &ResolutionCtx { controller: Some(PlayerId(0)), chosen_targets: vec![Target::Object(bears)], ..Default::default() }, WbReason::Resolve(StackId(0)));
        assert_eq!(e.state.computed(bears).power, Some(3), "target creature got a +1/+1 counter");
    }
}
