//! Child of Night — `{1}{B}` Creature — Vampire 2/1 with Lifelink (first printed M10, Magic 2010).

use crate::basics::Color;
use crate::cards::{grp, kw_creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType::*;


pub fn register(db: &mut CardDb) {
    db.insert(kw_creature(grp::CHILD_OF_NIGHT, "Child of Night", &[Vampire], Color::Black,
        mana_cost(1, &[(Color::Black, 1)]), 2, 1, vec![Keyword::Lifelink]).with_text("Lifelink"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn child_of_night_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::CHILD_OF_NIGHT).unwrap();
        assert_eq!(def.chars.power, Some(2));
        assert_eq!(def.chars.toughness, Some(1));
        expect![[r#"
            [
                Lifelink,
            ]"#]].assert_eq(&format!("{:#?}", def.chars.keywords));
    }
}
