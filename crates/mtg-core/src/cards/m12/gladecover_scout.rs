//! Gladecover Scout — `{G}` Creature — Elf Scout 1/1 with Hexproof (first printed M12, Magic 2012).

use crate::basics::Color;
use crate::cards::{grp, kw_creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType::*;


pub fn register(db: &mut CardDb) {
    db.insert(kw_creature(grp::GLADECOVER_SCOUT, "Gladecover Scout", &[Elf, Scout], Color::Green,
        mana_cost(0, &[(Color::Green, 1)]), 1, 1, vec![Keyword::Hexproof]).with_text("Hexproof"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn gladecover_scout_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::GLADECOVER_SCOUT).unwrap();
        assert_eq!(def.chars.power, Some(1));
        assert_eq!(def.chars.subtypes, vec![Elf.into(), Scout.into()]);
        expect![[r#"
            [
                Hexproof,
            ]"#]].assert_eq(&format!("{:#?}", def.chars.keywords));
    }
}
