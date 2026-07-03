//! Forum of Amity — Land (first printed SOS).
//!
//! Oracle: "This land enters tapped. / {T}: Add {W} or {B}. / {2}{W}{B}, {T}: Surveil 1."
//!
//! **Fully implemented** — the shared `surveil_dual` builder (W/B).

use crate::basics::Color;
use crate::cards::{surveil_dual, CardDb};

/// grp id (per-set ids live near their cards).
pub const FORUM_OF_AMITY: u32 = 250;

pub fn register(db: &mut CardDb) {
    db.insert(
        surveil_dual(FORUM_OF_AMITY, "Forum of Amity", Color::White, Color::Black)
            .with_text("This land enters tapped.\n{T}: Add {W} or {B}.\n{2}{W}{B}, {T}: Surveil 1."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;

    #[test]
    fn forum_of_amity_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(FORUM_OF_AMITY).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(def.is_mana_source());
        assert_eq!(def.abilities.len(), 4, "two mana abilities + enters-tapped + surveil ability");
        assert!(def.fully_implemented);
    }
}
