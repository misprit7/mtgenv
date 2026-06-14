//! Argothian Swine — `{3}{G}` Creature — Boar 3/3 with Trample (first printed USG, Urza's Saga).

use crate::basics::Color;
use crate::cards::{grp, kw_creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType::*;


pub fn register(db: &mut CardDb) {
    db.insert(kw_creature(grp::ARGOTHIAN_SWINE, "Argothian Swine", &[Boar], Color::Green,
        mana_cost(3, &[(Color::Green, 1)]), 3, 3, vec![Keyword::Trample]).with_text("Trample"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn argothian_swine_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::ARGOTHIAN_SWINE).unwrap();
        assert_eq!(def.chars.power, Some(3));
        assert_eq!(def.chars.toughness, Some(3));
        expect![[r#"
            [
                Trample,
            ]"#]].assert_eq(&format!("{:#?}", def.chars.keywords));
    }
}
