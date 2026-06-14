//! Grizzly Bears — `{1}{G}` Creature — Bear 2/2. A vanilla starter creature (first printed LEA).

use crate::basics::Color;
use crate::cards::{grp, mana_cost, vanilla_creature, CardDb};
use crate::subtypes::CreatureType::*;

pub fn register(db: &mut CardDb) {
    db.insert(vanilla_creature(
        grp::GRIZZLY_BEARS,
        "Grizzly Bears",
        &[Bear],
        Color::Green,
        mana_cost(1, &[(Color::Green, 1)]),
        2,
        2,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn grizzly_bears_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::GRIZZLY_BEARS).unwrap();
        assert_eq!(def.chars.power, Some(2));
        assert_eq!(def.chars.toughness, Some(2));
        expect!["[]"].assert_eq(&format!("{:#?}", def.abilities));
    }
}
