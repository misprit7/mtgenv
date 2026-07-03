//! Terramorphic Expanse — Land (first real-expansion printing TSP; reprinted in SOS).
//!
//! Oracle: "{T}, Sacrifice this land: Search your library for a basic land card, put it onto the
//! battlefield tapped, then shuffle."
//!
//! **Fully implemented** — a fetch land: `{T}, Sacrifice` → search a basic onto the battlefield
//! tapped (same machinery as Fabled Passage, without the land-count untap clause).

use crate::cards::helpers::{fetch_basic_tapped, sacrifice_self};
use crate::cards::{CardDb, CardDef};
use crate::basics::CardType;
use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
use crate::state::Characteristics;

/// grp id (per-set ids live near their cards).
pub const TERRAMORPHIC_EXPANSE: u32 = 277;

pub fn register(db: &mut CardDb) {
    let chars = Characteristics {
        name: "Terramorphic Expanse".to_string(),
        card_types: vec![CardType::Land],
        grp_id: TERRAMORPHIC_EXPANSE,
        ..Default::default()
    };
    db.insert(CardDef {
        chars,
        abilities: vec![Ability::Activated {
            cost: Cost {
                mana: None,
                components: vec![CostComponent::TapSelf, CostComponent::Sacrifice(sacrifice_self())],
            },
            effect: fetch_basic_tapped(),
            timing: Timing::Instant,
            restriction: None,
            is_mana: false,
        }],
        text: "{T}, Sacrifice this land: Search your library for a basic land card, put it onto the battlefield tapped, then shuffle.".to_string(),
        fully_implemented: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terramorphic_expanse_is_a_sac_fetch() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TERRAMORPHIC_EXPANSE).unwrap();
        assert!(def.chars.card_types.contains(&CardType::Land));
        assert!(def.fully_implemented);
        assert!(matches!(def.abilities[0], Ability::Activated { .. }));
    }
}
