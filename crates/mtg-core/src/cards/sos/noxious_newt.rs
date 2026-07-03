//! Noxious Newt — `{1}{G}` Creature — Salamander 1/2 (first printed SOS).
//!
//! Oracle: "Deathtouch / {T}: Add {G}."
//!
//! **Fully implemented** — printed Deathtouch (CR 702.2) plus a `{T}: Add {G}` mana ability
//! (CR 605), the canonical `mana_ability` builder.

use crate::basics::Color;
use crate::cards::{creature, mana_ability, mana_cost, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const NOXIOUS_NEWT: u32 = 213;

pub fn register(db: &mut CardDb) {
    let mut def = creature(
        NOXIOUS_NEWT,
        "Noxious Newt",
        &[CreatureType::Salamander],
        Color::Green,
        mana_cost(1, &[(Color::Green, 1)]),
        1,
        2,
        vec![mana_ability(Color::Green)],
    );
    def.chars.keywords = vec![Keyword::Deathtouch];
    def.text = "Deathtouch\n{T}: Add {G}.".to_string();
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noxious_newt_has_deathtouch_and_taps_for_green() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(NOXIOUS_NEWT).unwrap();
        assert_eq!(def.chars.power, Some(1));
        assert_eq!(def.chars.toughness, Some(2));
        assert_eq!(def.chars.keywords, vec![Keyword::Deathtouch]);
        assert!(def.is_mana_source(), "{{T}}: Add {{G}} mana ability");
        assert!(def.fully_implemented);
    }
}
