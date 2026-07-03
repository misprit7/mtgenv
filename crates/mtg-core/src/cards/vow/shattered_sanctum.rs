//! Shattered Sanctum — Land (first printed VOW; reprinted in SOS).
//!
//! Oracle: "This land enters tapped unless you control two or more other lands. / {T}: Add {W} or
//! {B}."
//!
//! **Fully implemented** — the shared `checkland` builder (W/B).

use crate::basics::Color;
use crate::cards::{checkland, CardDb};

/// grp id (per-set ids live near their cards).
pub const SHATTERED_SANCTUM: u32 = 245;

pub fn register(db: &mut CardDb) {
    db.insert(
        checkland(SHATTERED_SANCTUM, "Shattered Sanctum", Color::White, Color::Black)
            .with_text("This land enters tapped unless you control two or more other lands.\n{T}: Add {W} or {B}."),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basics::CardType;

    #[test]
    fn shattered_sanctum_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(SHATTERED_SANCTUM).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(def.is_mana_source());
        assert_eq!(def.abilities.len(), 3);
        assert!(def.fully_implemented);
    }
}
