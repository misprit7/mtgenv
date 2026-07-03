//! Strixhaven Skycoach — `{3}` Artifact — Vehicle 3/2 (first printed SOS).
//!
//! Oracle: "Flying / When this Vehicle enters, you may search your library for a basic land card,
//! reveal it, put it into your hand, then shuffle. / Crew 2"
//!
//! **Fully implemented** — a colorless Vehicle with Flying, an ETB "may fetch a basic to hand", and
//! Crew 2 (tap creatures with total power ≥ 2 → it becomes an artifact creature until end of turn).

use crate::basics::{CardType, Zone, ZoneDest, ZonePos};
use crate::cards::helpers::basic_land_filter;
use crate::cards::{mana_cost, CardDb, CardDef};
use crate::effects::ability::{Ability, Cost, CostComponent, EventPattern, Keyword, Timing};
use crate::effects::condition::Duration;
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};
use crate::state::Characteristics;
use crate::subtypes::ArtifactType;

/// grp id (per-set ids live near their cards).
pub const STRIXHAVEN_SKYCOACH: u32 = 276;

pub fn register(db: &mut CardDb) {
    let def = CardDef {
        chars: Characteristics {
            name: "Strixhaven Skycoach".to_string(),
            card_types: vec![CardType::Artifact],
            subtypes: vec![ArtifactType::Vehicle.into()],
            colors: vec![],
            mana_cost: Some(mana_cost(3, &[])),
            power: Some(3),
            toughness: Some(2),
            keywords: vec![Keyword::Flying],
            grp_id: STRIXHAVEN_SKYCOACH,
            ..Default::default()
        },
        abilities: vec![
            // "When this Vehicle enters, you may fetch a basic to hand."
            Ability::Triggered {
                event: EventPattern::SelfEnters,
                condition: None,
                intervening_if: false,
                effect: Effect::Optional {
                    prompt: "Search for a basic land card?".to_string(),
                    body: Box::new(Effect::Search {
                        who: PlayerRef::Controller,
                        zone: Zone::Library,
                        filter: basic_land_filter(),
                        min: 0,
                        max: 1,
                        to: ZoneDest { zone: Zone::Hand, pos: ZonePos::Any },
                        tapped: false,
                    }),
                },
            },
            // "Crew 2" — tap creatures with total power ≥ 2 → becomes an artifact creature until EOT.
            Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::Crew(2)] },
                effect: Effect::BecomeCreature { what: EffectTarget::SourceSelf, duration: Duration::UntilEndOfTurn },
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            },
        ],
        text: "Flying\nWhen this Vehicle enters, you may search your library for a basic land card, reveal it, put it into your hand, then shuffle.\nCrew 2".to_string(),
        fully_implemented: true,
    };
    db.insert(def);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strixhaven_skycoach_shape() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(STRIXHAVEN_SKYCOACH).unwrap();
        assert!(def.chars.card_types.contains(&CardType::Artifact));
        assert!(def.chars.subtypes.contains(&ArtifactType::Vehicle.into()));
        assert_eq!(def.chars.keywords, vec![Keyword::Flying]);
        assert_eq!((def.chars.power, def.chars.toughness), (Some(3), Some(2)));
        assert!(def.chars.colors.is_empty(), "colorless");
        assert!(matches!(def.abilities[1], Ability::Activated { .. }), "Crew ability");
        assert!(def.fully_implemented);
    }
}
