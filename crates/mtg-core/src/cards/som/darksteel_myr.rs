//! Darksteel Myr — `{3}` colorless Artifact Creature — Myr 0/1 with Indestructible (first printed
//! SOM, Scars of Mirrodin).

use crate::basics::{CardType, Color};
use crate::cards::{grp, kw_creature, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType::*;


pub fn register(db: &mut CardDb) {
    let mut myr = kw_creature(grp::DARKSTEEL_MYR, "Darksteel Myr", &[Myr], Color::White,
        mana_cost(3, &[]), 0, 1, vec![Keyword::Indestructible]);
    myr.chars.card_types = vec![CardType::Artifact, CardType::Creature];
    myr.chars.colors = Vec::new();
    db.insert(myr.with_text("Indestructible"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn darksteel_myr_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::DARKSTEEL_MYR).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Artifact, CardType::Creature]);
        assert!(def.chars.colors.is_empty());
        expect![[r#"
            [
                Indestructible,
            ]"#]].assert_eq(&format!("{:#?}", def.chars.keywords));
    }
}
