//! Llanowar Elves — `{G}` Creature — Elf Druid 1/1 (first printed LEA). "{T}: Add {G}."
//!
//! A green mana dork. The mana ability is represented engine-side via `mana_colors` (the same
//! "{T}: add one of these colours" slot basic lands use); the engine gates it by summoning
//! sickness (C1, CR 302.6) so a freshly-cast Llanowar can't tap the turn it enters.

use crate::basics::Color;
use crate::cards::{creature, mana_cost, CardDb};

/// grp id (per-set ids live near their cards).
pub const LLANOWAR_ELVES: u32 = 100;

pub fn register(db: &mut CardDb) {
    let mut elf = creature(
        LLANOWAR_ELVES,
        "Llanowar Elves",
        "Elf",
        Color::Green,
        mana_cost(0, &[(Color::Green, 1)]),
        1,
        1,
        Vec::new(),
    );
    elf.chars.subtypes = vec!["Elf".to_string(), "Druid".to_string()];
    elf.mana_colors = vec![Color::Green];
    db.insert(elf.with_text("{T}: Add {G}."));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn llanowar_elves_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(LLANOWAR_ELVES).unwrap();
        // A 1/1 Elf Druid that taps for {G}; no Effect-IR abilities (the mana ability is the
        // engine-side `mana_colors` slot).
        assert_eq!(def.chars.power, Some(1));
        assert_eq!(def.chars.toughness, Some(1));
        assert_eq!(def.chars.subtypes, vec!["Elf".to_string(), "Druid".to_string()]);
        assert_eq!(def.mana_colors, vec![Color::Green]);
        assert!(def.abilities.is_empty());
        expect![["{T}: Add {G}."]].assert_eq(&def.text);
    }
}
