//! Shock — `{R}` Instant. "Shock deals 2 damage to any target." (first printed STH, Stronghold).

use crate::basics::{CardType, Color};
use crate::cards::{grp, deal_to_any, mana_cost, spell, CardDb};


pub fn register(db: &mut CardDb) {
    db.insert(
        spell(
            grp::SHOCK,
            "Shock",
            CardType::Instant,
            Color::Red,
            mana_cost(0, &[(Color::Red, 1)]),
            deal_to_any(2),
        )
        .with_text("Shock deals 2 damage to any target."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn shock_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::SHOCK).unwrap();
        assert!(def.spell_effect().is_some());
        expect![[r#"
            [
                Spell {
                    effect: DealDamage {
                        amount: Fixed(
                            2,
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
