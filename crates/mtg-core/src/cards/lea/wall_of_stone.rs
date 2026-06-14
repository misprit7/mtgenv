//! Wall of Stone — `{1}{R}{R}` Creature — Wall 0/8 with Defender (first printed LEA).

use crate::basics::Color;
use crate::cards::{grp, kw_creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType::*;

pub fn register(db: &mut CardDb) {
    db.insert(
        kw_creature(
            grp::WALL_OF_STONE,
            "Wall of Stone",
            &[Wall],
            Color::Red,
            mana_cost(1, &[(Color::Red, 2)]),
            0,
            8,
            vec![Keyword::Defender],
        )
        .with_text("Defender"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn wall_of_stone_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::WALL_OF_STONE).unwrap();
        assert_eq!(def.chars.power, Some(0));
        assert_eq!(def.chars.toughness, Some(8));
        expect![[r#"
            [
                Defender,
            ]"#]].assert_eq(&format!("{:#?}", def.chars.keywords));
    }
}
