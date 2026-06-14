//! Fencing Ace — `{1}{W}` Creature — Human Soldier 1/1 with Double strike (first printed RTR,
//! Return to Ravnica).

use crate::basics::Color;
use crate::cards::{grp, kw_creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType::*;


pub fn register(db: &mut CardDb) {
    db.insert(kw_creature(grp::FENCING_ACE, "Fencing Ace", &[Human, Soldier], Color::White,
        mana_cost(1, &[(Color::White, 1)]), 1, 1, vec![Keyword::DoubleStrike]).with_text("Double strike"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn fencing_ace_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::FENCING_ACE).unwrap();
        assert_eq!(def.chars.power, Some(1));
        assert_eq!(def.chars.subtypes, vec![Human.into(), Soldier.into()]);
        expect![[r#"
            [
                DoubleStrike,
            ]"#]].assert_eq(&format!("{:#?}", def.chars.keywords));
    }
}
