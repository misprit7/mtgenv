//! Shared card-construction fragments — reusable `CardFilter` / `SelectSpec` / `ValueExpr` pieces
//! that more than one card needs.
//!
//! **Rule (code-org law):** a card module (`<setcode>/<card>.rs` or `misc/*.rs`) must NEVER import
//! from a *sibling* card module. Anything shared between cards lives **here** (or, for whole
//! `CardDef`/`Ability` builders like `creature`/`mana_ability`, in the parent `crate::cards`).
//! That keeps every card module a leaf node — no card-to-card tangle as the pool grows.

use crate::basics::{CardType, Zone};
use crate::effects::target::{CardFilter, SelectSpec};
use crate::effects::value::{PlayerRef, ValueExpr};

/// "a land you control" — the landfall event filter (CR 603.2: a land entering under your
/// control). Shared by every landfall trigger (Sazh's Chocobo, Mossborn Hydra, Icetill Explorer…).
pub(crate) fn land_you_control() -> CardFilter {
    CardFilter::All(vec![
        CardFilter::HasCardType(CardType::Land),
        CardFilter::ControlledBy(PlayerRef::Controller),
    ])
}

/// "the number of lands you control" — a dynamic count (e.g. Lumbering Worldwagon's `*` power).
/// The `controller` field restricts to you; the filter narrows to lands (matches the chars-layer
/// 7a CDA form the engine evaluates).
pub(crate) fn lands_you_control() -> ValueExpr {
    ValueExpr::Count {
        zone: Zone::Battlefield,
        filter: CardFilter::All(vec![
            CardFilter::HasCardType(CardType::Land),
            CardFilter::ControlledBy(PlayerRef::Controller),
        ]),
        controller: Some(PlayerRef::Controller),
    }
}

/// `CardFilter` matching a basic land card (CR 205.4b) — `All([Land, Supertype("Basic")])`.
/// Shared by every "search your library for a basic land card" effect (fetch lands, Bushwhack,
/// Lumbering Worldwagon…).
pub(crate) fn basic_land_filter() -> CardFilter {
    CardFilter::All(vec![
        CardFilter::HasCardType(CardType::Land),
        CardFilter::Supertype("Basic".to_string()),
    ])
}

/// `SelectSpec` for a static affecting "creatures you control" (the anthem scope). min/max are
/// unused for statics (they apply to every match).
pub(crate) fn creatures_you_control() -> SelectSpec {
    SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::All(vec![
            CardFilter::HasCardType(CardType::Creature),
            CardFilter::ControlledBy(PlayerRef::Controller),
        ]),
        chooser: PlayerRef::Controller,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(0),
    }
}

/// `SelectSpec` for a static affecting "the permanent this Aura/Equipment is attached to"
/// (CR 702.3e/702.6e) — the source-relative `AttachedHost` filter. min/max unused for statics.
pub(crate) fn attached_host() -> SelectSpec {
    SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::AttachedHost,
        chooser: PlayerRef::Controller,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(0),
    }
}

/// `SelectSpec` matching "this object itself" (the `ItSelf` filter) — for a self-only static such
/// as a characteristic-defining `*`/N (Lumbering Worldwagon). min/max unused for statics.
pub(crate) fn itself() -> SelectSpec {
    SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::ItSelf,
        chooser: PlayerRef::Controller,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(0),
    }
}
