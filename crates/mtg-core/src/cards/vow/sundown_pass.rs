//! Sundown Pass — Land (first printed VOW; reprinted in SOS).
//!
//! Oracle: "This land enters tapped unless you control two or more other lands. / {T}: Add {R} or
//! {W}."
//!
//! **Fully implemented** — the shared `checkland` builder (R/W).

use crate::basics::Color;
use crate::cards::{checkland, CardDb};

/// grp id (per-set ids live near their cards).
pub const SUNDOWN_PASS: u32 = 247;

pub fn register(db: &mut CardDb) {
    db.insert(
        checkland(SUNDOWN_PASS, "Sundown Pass", Color::Red, Color::White)
            .with_text("This land enters tapped unless you control two or more other lands.\n{T}: Add {R} or {W}."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;

    #[test]
    fn sundown_pass_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SUNDOWN_PASS).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(def.is_mana_source());
        assert_eq!(def.abilities.len(), 3);
        assert!(def.fully_implemented);
    }
}
