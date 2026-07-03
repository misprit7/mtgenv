//! Paradox Gardens — Land (first printed SOS).
//!
//! Oracle: "This land enters tapped. / {T}: Add {G} or {U}. / {2}{G}{U}, {T}: Surveil 1."
//!
//! **Fully implemented** — the shared `surveil_dual` builder (G/U).

use crate::basics::Color;
use crate::cards::{surveil_dual, CardDb};

/// grp id (per-set ids live near their cards).
pub const PARADOX_GARDENS: u32 = 251;

pub fn register(db: &mut CardDb) {
    db.insert(
        surveil_dual(PARADOX_GARDENS, "Paradox Gardens", Color::Green, Color::Blue)
            .with_text("This land enters tapped.\n{T}: Add {G} or {U}.\n{2}{G}{U}, {T}: Surveil 1."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;

    #[test]
    fn paradox_gardens_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(PARADOX_GARDENS).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(def.is_mana_source());
        assert_eq!(def.abilities.len(), 4, "two mana abilities + enters-tapped + surveil ability");
        assert!(def.fully_implemented);
    }
}
