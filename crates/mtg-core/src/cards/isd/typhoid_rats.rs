//! Typhoid Rats — `{B}` Creature — Rat 1/1 with Deathtouch (first printed ISD, Innistrad).

use crate::basics::Color;
use crate::cards::{grp, kw_creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType::*;


pub fn register(db: &mut CardDb) {
    db.insert(kw_creature(grp::TYPHOID_RATS, "Typhoid Rats", &[Rat], Color::Black,
        mana_cost(0, &[(Color::Black, 1)]), 1, 1, vec![Keyword::Deathtouch]).with_text("Deathtouch"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn typhoid_rats_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::TYPHOID_RATS).unwrap();
        assert_eq!(def.chars.power, Some(1));
        assert_eq!(def.chars.toughness, Some(1));
        expect![[r#"
            [
                Deathtouch,
            ]"#]].assert_eq(&format!("{:#?}", def.chars.keywords));
    }
}
