//! Elvish Archers — `{1}{G}` Creature — Elf Archer 2/1 with First strike (first printed LEA).

use crate::basics::Color;
use crate::cards::{grp, kw_creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType::*;

pub fn register(db: &mut CardDb) {
    db.insert(
        kw_creature(
            grp::ELVISH_ARCHERS,
            "Elvish Archers",
            &[Elf, Archer],
            Color::Green,
            mana_cost(1, &[(Color::Green, 1)]),
            2,
            1,
            vec![Keyword::FirstStrike],
        )
        .with_text("First strike"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn elvish_archers_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::ELVISH_ARCHERS).unwrap();
        assert_eq!(def.chars.power, Some(2));
        assert_eq!(def.chars.toughness, Some(1));
        assert_eq!(def.chars.subtypes, vec![Elf.into(), Archer.into()]);
        expect![[r#"
            [
                FirstStrike,
            ]"#]].assert_eq(&format!("{:#?}", def.chars.keywords));
    }
}
