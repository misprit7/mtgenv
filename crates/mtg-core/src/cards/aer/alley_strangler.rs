//! Alley Strangler — `{2}{B}` Creature — Human Assassin 2/3 with Menace (first printed AER,
//! Aether Revolt).

use crate::basics::Color;
use crate::cards::{grp, kw_creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType::*;


pub fn register(db: &mut CardDb) {
    db.insert(kw_creature(grp::ALLEY_STRANGLER, "Alley Strangler", &[Human, Assassin], Color::Black,
        mana_cost(2, &[(Color::Black, 1)]), 2, 3, vec![Keyword::Menace]).with_text("Menace"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn alley_strangler_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::ALLEY_STRANGLER).unwrap();
        assert_eq!(def.chars.power, Some(2));
        assert_eq!(def.chars.toughness, Some(3));
        assert_eq!(def.chars.subtypes, vec![Human.into(), Assassin.into()]);
        expect![[r#"
            [
                Menace,
            ]"#]].assert_eq(&format!("{:#?}", def.chars.keywords));
    }
}
