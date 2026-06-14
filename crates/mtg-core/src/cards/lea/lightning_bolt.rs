//! Lightning Bolt — `{R}` Instant. "Lightning Bolt deals 3 damage to any target." (first printed LEA).

use crate::basics::{CardType, Color};
use crate::cards::{deal_to_any, grp, mana_cost, spell, CardDb};

pub fn register(db: &mut CardDb) {
    db.insert(
        spell(
            grp::LIGHTNING_BOLT,
            "Lightning Bolt",
            CardType::Instant,
            Color::Red,
            mana_cost(0, &[(Color::Red, 1)]),
            deal_to_any(3),
        )
        .with_text("Lightning Bolt deals 3 damage to any target."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn lightning_bolt_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::LIGHTNING_BOLT).unwrap();
        assert!(def.spell_effect().is_some());
        expect![[r#"
            [
                Spell {
                    effect: DealDamage {
                        amount: Fixed(
                            3,
                        ),
                        to: Target(
                            TargetSpec {
                                kind: Any,
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
