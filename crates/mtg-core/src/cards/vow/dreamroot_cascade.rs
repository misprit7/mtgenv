//! Dreamroot Cascade — Land (first printed VOW; reprinted in SOS).
//!
//! Oracle: "This land enters tapped unless you control two or more other lands. / {T}: Add {G} or
//! {U}."
//!
//! **Fully implemented** — the shared `checkland` builder (G/U).

use crate::basics::Color;
use crate::cards::{checkland, CardDb};

/// grp id (per-set ids live near their cards).
pub const DREAMROOT_CASCADE: u32 = 244;

pub fn register(db: &mut CardDb) {
    db.insert(
        checkland(DREAMROOT_CASCADE, "Dreamroot Cascade", Color::Green, Color::Blue)
            .with_text("This land enters tapped unless you control two or more other lands.\n{T}: Add {G} or {U}."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;

    #[test]
    fn dreamroot_cascade_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(DREAMROOT_CASCADE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(def.is_mana_source());
        assert_eq!(def.abilities.len(), 3, "two mana abilities + the enters-tapped replacement");
        assert!(def.fully_implemented);
    }
}
