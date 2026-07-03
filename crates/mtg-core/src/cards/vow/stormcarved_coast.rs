//! Stormcarved Coast — Land (first printed VOW; reprinted in SOS).
//!
//! Oracle: "This land enters tapped unless you control two or more other lands. / {T}: Add {U} or
//! {R}."
//!
//! **Fully implemented** — the shared `checkland` builder (U/R).

use crate::basics::Color;
use crate::cards::{checkland, CardDb};

/// grp id (per-set ids live near their cards).
pub const STORMCARVED_COAST: u32 = 246;

pub fn register(db: &mut CardDb) {
    db.insert(
        checkland(STORMCARVED_COAST, "Stormcarved Coast", Color::Blue, Color::Red)
            .with_text("This land enters tapped unless you control two or more other lands.\n{T}: Add {U} or {R}."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;

    #[test]
    fn stormcarved_coast_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(STORMCARVED_COAST).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(def.is_mana_source());
        assert_eq!(def.abilities.len(), 3);
        assert!(def.fully_implemented);
    }
}
