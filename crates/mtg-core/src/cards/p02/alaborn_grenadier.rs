//! Alaborn Grenadier — `{2}{W}` Creature — Human Soldier 2/2 with Vigilance (first printed P02,
//! Portal Second Age).

use crate::basics::Color;
use crate::cards::{grp, kw_creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType::*;


pub fn register(db: &mut CardDb) {
    db.insert(kw_creature(grp::ALABORN_GRENADIER, "Alaborn Grenadier", &[Human, Soldier], Color::White,
        mana_cost(0, &[(Color::White, 2)]), 2, 2, vec![Keyword::Vigilance]).with_text("Vigilance"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn alaborn_grenadier_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::ALABORN_GRENADIER).unwrap();
        assert_eq!(def.chars.power, Some(2));
        assert_eq!(def.chars.subtypes, vec![Human.into(), Soldier.into()]);
        expect![[r#"
            [
                Vigilance,
            ]"#]].assert_eq(&format!("{:#?}", def.chars.keywords));
    }
}
