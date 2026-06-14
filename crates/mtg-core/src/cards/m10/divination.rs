//! Divination — `{2}{U}` Sorcery. "Draw two cards." (first printed M10, Magic 2010).

use crate::basics::{CardType, Color};
use crate::cards::{grp, mana_cost, spell, CardDb};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;


pub fn register(db: &mut CardDb) {
    db.insert(
        spell(
            grp::DIVINATION,
            "Divination",
            CardType::Sorcery,
            Color::Blue,
            mana_cost(2, &[(Color::Blue, 1)]),
            Effect::Draw {
                who: PlayerRef::Controller,
                count: ValueExpr::Fixed(2),
            },
        )
        .with_text("Draw two cards."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn divination_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::DIVINATION).unwrap();
        assert!(def.spell_effect().is_some());
        expect![[r#"
            [
                Spell {
                    effect: Draw {
                        who: Controller,
                        count: Fixed(
                            2,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
