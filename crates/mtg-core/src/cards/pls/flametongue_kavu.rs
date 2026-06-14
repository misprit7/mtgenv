//! Flametongue Kavu — `{3}{R}` Creature — Kavu 4/2. "When this creature enters, it deals 4
//! damage to target creature." (ETB trigger that targets — chosen as it goes on the stack, CR
//! 603.3d.) First printed PLS (Planeshift).

use crate::basics::{Color, DamageKind};
use crate::cards::{grp, creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::ValueExpr;
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;


pub fn register(db: &mut CardDb) {
    db.insert(creature(
        grp::FLAMETONGUE_KAVU,
        "Flametongue Kavu",
        &[CreatureType::Kavu],
        Color::Red,
        mana_cost(3, &[(Color::Red, 1)]),
        4,
        2,
        vec![Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            effect: Effect::DealDamage {
                amount: ValueExpr::Fixed(4),
                to: EffectTarget::Target(TargetSpec {
                    kind: TargetKind::Creature(CardFilter::Any),
                    min: 1,
                    max: 1,
                    distinct: true,
                }),
                kind: DamageKind::Noncombat,
            },
        }],
    ).with_text("When this creature enters, it deals 4 damage to target creature."));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn flametongue_kavu_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::FLAMETONGUE_KAVU).unwrap();
        assert_eq!(def.chars.power, Some(4));
        assert_eq!(def.chars.toughness, Some(2));
        expect![[r#"
            [
                Triggered {
                    event: SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: DealDamage {
                        amount: Fixed(
                            4,
                        ),
                        to: Target(
                            TargetSpec {
                                kind: Creature(
                                    Any,
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                        kind: Noncombat,
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
