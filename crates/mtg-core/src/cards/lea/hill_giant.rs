//! Hill Giant — `{3}{R}` Creature — Giant 3/3. A vanilla starter creature (first printed LEA).

use crate::basics::Color;
use crate::cards::{grp, mana_cost, vanilla_creature, CardDb};
use crate::subtypes::CreatureType::*;

pub fn register(db: &mut CardDb) {
    db.insert(vanilla_creature(
        grp::HILL_GIANT,
        "Hill Giant",
        &[Giant],
        Color::Red,
        mana_cost(3, &[(Color::Red, 1)]),
        3,
        3,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn hill_giant_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::HILL_GIANT).unwrap();
        assert_eq!(def.chars.power, Some(3));
        assert_eq!(def.chars.toughness, Some(3));
        expect!["[]"].assert_eq(&format!("{:#?}", def.abilities));
    }
}
