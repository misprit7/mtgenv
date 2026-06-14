//! King Cheetah — `{3}{G}` Creature — Cat 3/2 with Flash (first printed MGB, Multiverse Gift Box).

use crate::basics::Color;
use crate::cards::{grp, kw_creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType::*;


pub fn register(db: &mut CardDb) {
    db.insert(kw_creature(grp::KING_CHEETAH, "King Cheetah", &[Cat], Color::Green,
        mana_cost(3, &[(Color::Green, 1)]), 3, 2, vec![Keyword::Flash]).with_text("Flash"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn king_cheetah_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::KING_CHEETAH).unwrap();
        assert_eq!(def.chars.power, Some(3));
        assert_eq!(def.chars.toughness, Some(2));
        expect![[r#"
            [
                Flash,
            ]"#]].assert_eq(&format!("{:#?}", def.chars.keywords));
    }
}
