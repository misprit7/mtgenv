//! Fields of Strife — Land (first printed SOS).
//!
//! Oracle: "This land enters tapped. / {T}: Add {R} or {W}. / {2}{R}{W}, {T}: Surveil 1."
//!
//! **Fully implemented** — the shared `surveil_dual` builder (R/W).

use crate::basics::Color;
use crate::cards::{surveil_dual, CardDb};

/// grp id (per-set ids live near their cards).
pub const FIELDS_OF_STRIFE: u32 = 249;

pub fn register(db: &mut CardDb) {
    db.insert(
        surveil_dual(FIELDS_OF_STRIFE, "Fields of Strife", Color::Red, Color::White)
            .with_text("This land enters tapped.\n{T}: Add {R} or {W}.\n{2}{R}{W}, {T}: Surveil 1."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;

    #[test]
    fn fields_of_strife_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(FIELDS_OF_STRIFE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(def.is_mana_source());
        assert_eq!(def.abilities.len(), 4, "two mana abilities + enters-tapped + surveil ability");
        assert!(def.fully_implemented);
    }
}
