//! Spectacle Summit — Land (first printed SOS).
//!
//! Oracle: "This land enters tapped. / {T}: Add {U} or {R}. / {2}{U}{R}, {T}: Surveil 1."
//!
//! **Fully implemented** — the shared `surveil_dual` builder (U/R).

use crate::basics::Color;
use crate::cards::{surveil_dual, CardDb};

/// grp id (per-set ids live near their cards).
pub const SPECTACLE_SUMMIT: u32 = 252;

pub fn register(db: &mut CardDb) {
    db.insert(
        surveil_dual(SPECTACLE_SUMMIT, "Spectacle Summit", Color::Blue, Color::Red)
            .with_text("This land enters tapped.\n{T}: Add {U} or {R}.\n{2}{U}{R}, {T}: Surveil 1."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;

    #[test]
    fn spectacle_summit_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SPECTACLE_SUMMIT).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(def.is_mana_source());
        assert_eq!(def.abilities.len(), 4, "two mana abilities + enters-tapped + surveil ability");
        assert!(def.fully_implemented);
    }
}
