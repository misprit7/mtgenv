//! Icetill Explorer — `{2}{G}{G}` Creature — Insect Scout 2/4 (first printed EOE, Edge of Eternities).
//!
//! Oracle:
//!   You may play an additional land on each of your turns.
//!   You may play lands from your graveyard.
//!   Landfall — Whenever a land you control enters, mill a card.
//!
//! **Fully implemented** (C18 land-play permissions landed, cap `3ca7fef`):
//! - "You may play an additional land on each of your turns." — `Ability::Static{
//!   StaticContribution::ExtraLandPlays(1) }` (the land-play limit = 1 + Σ extra plays, CR 505.5b).
//! - "You may play lands from your graveyard." — `Ability::Static{ PlayLandsFrom(Zone::Graveyard) }`
//!   (the land-play legality offers graveyard lands while this is in play).
//! - "Landfall — Whenever a land you control enters, mill a card." — `Triggered{PermanentEnters(land
//!   you control)}` → `Mill` (C4 + C3).
//! (These two permissions are player-level statics: the engine reads them directly from the
//! controller's permanents in the land-play legality, not painted on objects — so `affects: itself()`.)

use crate::basics::{Color, Zone};
use crate::cards::helpers::{itself, land_you_control};
use crate::cards::{creature, mana_cost, CardDb};
use crate::effects::ability::{Ability, EventPattern, StaticContribution};
use crate::effects::condition::Duration;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::Effect;
use crate::subtypes::CreatureType;

/// grp id (per-set ids live near their cards).
pub const ICETILL_EXPLORER: u32 = 104;

pub fn register(db: &mut CardDb) {
    let explorer = creature(
        ICETILL_EXPLORER,
        "Icetill Explorer",
        &[CreatureType::Insect, CreatureType::Scout],
        Color::Green,
        mana_cost(2, &[(Color::Green, 2)]),
        2,
        4,
        vec![
            // "You may play an additional land on each of your turns."
            Ability::Static {
                contribution: StaticContribution::ExtraLandPlays(1),
                affects: itself(),
                duration: Duration::WhileSourcePresent,
            },
            // "You may play lands from your graveyard."
            Ability::Static {
                contribution: StaticContribution::PlayLandsFrom(Zone::Graveyard),
                affects: itself(),
                duration: Duration::WhileSourcePresent,
            },
            // "Landfall — Whenever a land you control enters, mill a card."
            Ability::Triggered {
                event: EventPattern::PermanentEnters(land_you_control()),
                condition: None,
                intervening_if: false,
                effect: Effect::Mill {
                    who: PlayerRef::Controller,
                    count: ValueExpr::Fixed(1),
                },
            },
        ],
    );
    db.insert(explorer.with_text(
        "You may play an additional land on each of your turns.\nYou may play lands from your graveyard.\nLandfall — Whenever a land you control enters, mill a card.",
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn icetill_explorer_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(ICETILL_EXPLORER).unwrap();
        assert_eq!(def.chars.power, Some(2));
        assert_eq!(def.chars.toughness, Some(4));
        assert_eq!(def.chars.subtypes, vec![CreatureType::Insect.into(), CreatureType::Scout.into()]);
        assert!(!def.is_mana_source());
        assert!(def.fully_implemented); // two land-play statics (C18) + landfall mill
        // Two land-play permission statics (ExtraLandPlays + PlayLandsFrom Graveyard) + landfall mill.
        expect![[r#"
            [
                Static {
                    contribution: ExtraLandPlays(
                        1,
                    ),
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: ItSelf,
                        chooser: Controller,
                        min: Fixed(
                            0,
                        ),
                        max: Fixed(
                            0,
                        ),
                    },
                    duration: WhileSourcePresent,
                },
                Static {
                    contribution: PlayLandsFrom(
                        Graveyard,
                    ),
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: ItSelf,
                        chooser: Controller,
                        min: Fixed(
                            0,
                        ),
                        max: Fixed(
                            0,
                        ),
                    },
                    duration: WhileSourcePresent,
                },
                Triggered {
                    event: PermanentEnters(
                        All(
                            [
                                HasCardType(
                                    Land,
                                ),
                                ControlledBy(
                                    Controller,
                                ),
                            ],
                        ),
                    ),
                    condition: None,
                    intervening_if: false,
                    effect: Mill {
                        who: Controller,
                        count: Fixed(
                            1,
                        ),
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
