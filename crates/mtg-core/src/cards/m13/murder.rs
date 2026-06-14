//! Murder — `{1}{B}{B}` Instant. "Destroy target creature." (first printed M13, Magic 2013).

use crate::basics::{CardType, Color};
use crate::cards::{grp, mana_cost, spell, CardDb};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::{Effect, EffectTarget};


pub fn register(db: &mut CardDb) {
    db.insert(
        spell(
            grp::MURDER,
            "Murder",
            CardType::Instant,
            Color::Black,
            mana_cost(1, &[(Color::Black, 2)]),
            Effect::Destroy {
                what: EffectTarget::Target(TargetSpec {
                    kind: TargetKind::Creature(CardFilter::Any),
                    min: 1,
                    max: 1,
                    distinct: true,
                }),
            },
        )
        .with_text("Destroy target creature."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn murder_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::MURDER).unwrap();
        assert!(def.spell_effect().is_some());
        expect![[r#"
            [
                Spell {
                    effect: Destroy {
                        what: Target(
                            TargetSpec {
                                kind: Creature(
                                    Any,
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
