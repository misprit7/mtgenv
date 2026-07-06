//! Crop Rotation — `{G}` Instant (first printed ULG; reprinted on the SOS Mystical Archive `soa`).
//!
//! Oracle: "As an additional cost to cast this spell, sacrifice a land. Search your library for a
//! land card, put that card onto the battlefield, then shuffle."
//!
//! **Fully implemented** — a spell-level additional "sacrifice a land" cost (`Ability::AdditionalCost`,
//! paid at cast) + a `Search` that puts any land card from the library onto the battlefield (untapped,
//! then shuffles).

use crate::basics::{CardType, Color, Zone, ZoneDest, ZonePos};
use crate::cards::{mana_cost, spell, CardDb};
use crate::effects::ability::{AdditionalCost, Ability, Cost, CostComponent};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;

/// grp id (bonus-sheet `soa` cards use the 600+ block).
pub const CROP_ROTATION: u32 = 614;

pub fn register(db: &mut CardDb) {
    let effect = Effect::Search {
        who: PlayerRef::Controller,
        zone: Zone::Library,
        filter: CardFilter::HasCardType(CardType::Land),
        min: 0,
        max: 1,
        to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
        tapped: false,
    };
    let mut def = spell(
        CROP_ROTATION,
        "Crop Rotation",
        CardType::Instant,
        Color::Green,
        mana_cost(0, &[(Color::Green, 1)]),
        effect,
    )
    .with_text("As an additional cost to cast this spell, sacrifice a land.\nSearch your library for a land card, put that card onto the battlefield, then shuffle.");
    // "As an additional cost to cast this spell, sacrifice a land." (CR 601.2b)
    def.abilities.push(Ability::AdditionalCost(AdditionalCost {
        options: vec![Cost {
            mana: None,
            components: vec![CostComponent::Sacrifice(SelectSpec {
                zone: Zone::Battlefield,
                filter: CardFilter::All(vec![
                    CardFilter::HasCardType(CardType::Land),
                    CardFilter::ControlledBy(PlayerRef::Controller),
                ]),
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(1),
                max: ValueExpr::Fixed(1),
            })],
        }],
    }));
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::ability::CostComponent;

    #[test]
    fn crop_rotation_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(CROP_ROTATION).unwrap();
        assert!(def.fully_implemented);
        let ac = def.additional_costs();
        assert_eq!(ac.len(), 1, "one additional-cost clause (sacrifice a land)");
        assert!(matches!(ac[0].options[0].components[0], CostComponent::Sacrifice(_)));
        assert!(matches!(def.spell_effect().unwrap(), Effect::Search { .. }));
    }
}
