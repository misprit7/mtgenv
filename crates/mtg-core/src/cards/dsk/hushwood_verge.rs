//! Hushwood Verge — Land (first printed DSK). "{T}: Add {G}. {T}: Add {W}. Activate only if you
//! control a Forest or a Plains." A Selesnya (G/W) dual land.
//!
//! Modeled as a land that taps for {G} or {W} via the engine-side `mana_colors` slot.
//! deferred: the {W} ability's "only if you control a Forest or a Plains" condition — the current
//! mana representation can't express a *conditional* mana ability, so {W} is always available
//! here (a slight upgrade over the printed card). Revisit when conditional mana lands.

use crate::basics::{CardType, Color};
use crate::cards::{CardDb, CardDef};
use crate::state::Characteristics;

pub const HUSHWOOD_VERGE: u32 = 101;

pub fn register(db: &mut CardDb) {
    let chars = Characteristics {
        name: "Hushwood Verge".to_string(),
        card_types: vec![CardType::Land],
        grp_id: HUSHWOOD_VERGE,
        ..Default::default()
    };
    db.insert(
        CardDef {
            chars,
            abilities: Vec::new(),
            mana_colors: vec![Color::Green, Color::White],
            text: String::new(),
        }
        .with_text(
            "{T}: Add {G}.\n{T}: Add {W}. Activate only if you control a Forest or a Plains. \
             (Condition not yet modeled — {W} is always available here.)",
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hushwood_verge_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(HUSHWOOD_VERGE).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        assert!(def.chars.mana_cost.is_none()); // lands aren't cast
        assert_eq!(def.mana_colors, vec![Color::Green, Color::White]);
        assert!(def.abilities.is_empty());
        assert!(def.text.contains("not yet modeled")); // the deferred {W} condition
    }
}
