//! Raging Goblin — `{R}` Creature — Goblin 1/1 with Haste (first printed POR, Portal).

use crate::basics::Color;
use crate::cards::{grp, kw_creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType::*;


pub fn register(db: &mut CardDb) {
    db.insert(kw_creature(grp::RAGING_GOBLIN, "Raging Goblin", &[Goblin], Color::Red,
        mana_cost(0, &[(Color::Red, 1)]), 1, 1, vec![Keyword::Haste]).with_text("Haste"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn raging_goblin_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::RAGING_GOBLIN).unwrap();
        assert_eq!(def.chars.power, Some(1));
        assert_eq!(def.chars.toughness, Some(1));
        expect![[r#"
            [
                Haste,
            ]"#]].assert_eq(&format!("{:#?}", def.chars.keywords));
    }
}
