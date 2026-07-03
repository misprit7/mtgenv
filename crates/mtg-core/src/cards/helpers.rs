//! Shared card-construction fragments — reusable `CardFilter` / `SelectSpec` / `ValueExpr` pieces
//! that more than one card needs.
//!
//! **Rule (code-org law):** a card module (`<setcode>/<card>.rs` or `misc/*.rs`) must NEVER import
//! from a *sibling* card module. Anything shared between cards lives **here** (or, for whole
//! `CardDef`/`Ability` builders like `creature`/`mana_ability`, in the parent `crate::cards`).
//! That keeps every card module a leaf node — no card-to-card tangle as the pool grows.

use crate::basics::{CardType, Color, CounterKind, Zone, ZoneDest, ZonePos};
use crate::effects::ability::Keyword;
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec, TokenSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::{CreatureType, Supertype};

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

/// "an instant or sorcery spell" — `AnyOf([Instant, Sorcery])`. Shared by the SoS Opus / Repartee
/// cast-trigger cycles ("whenever you cast an instant or sorcery spell").
pub(crate) fn instant_or_sorcery() -> CardFilter {
    CardFilter::AnyOf(vec![
        CardFilter::HasCardType(CardType::Instant),
        CardFilter::HasCardType(CardType::Sorcery),
    ])
}

/// `CardFilter` matching a basic land card (CR 205.4b) — `All([Land, Supertype(Basic)])`.
/// Shared by every "search your library for a basic land card" effect (fetch lands, Bushwhack,
/// Lumbering Worldwagon…).
pub(crate) fn basic_land_filter() -> CardFilter {
    CardFilter::All(vec![
        CardFilter::HasCardType(CardType::Land),
        CardFilter::Supertype(Supertype::Basic),
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

/// `SelectSpec` for "sacrifice this" — exactly one object, the source itself (`ItSelf`). Used as a
/// `CostComponent::Sacrifice` payload, e.g. fetch lands' `{T}, Sacrifice this:`.
pub(crate) fn sacrifice_self() -> SelectSpec {
    SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::ItSelf,
        chooser: PlayerRef::Controller,
        min: ValueExpr::Fixed(1),
        max: ValueExpr::Fixed(1),
    }
}

/// "Search your library for a basic land card, put it onto the battlefield tapped, then shuffle"
/// (C5), searched by the controller — the fetch shared by fetch lands (Fabled Passage, Escape
/// Tunnel) and Lumbering Worldwagon. `min: 0` allows a failed/declined find; the engine shuffles after.
pub(crate) fn fetch_basic_tapped() -> Effect {
    fetch_basic_tapped_by(PlayerRef::Controller)
}

/// The Strixhaven "Inkling" token (CR 111.3): a 1/1 white-and-black Inkling with flying. Shared by
/// every card that makes one (Eager Glyphmage, Harsh Annotation, Informed Inkwright, …).
pub(crate) fn inkling_token() -> TokenSpec {
    TokenSpec {
        name: "Inkling".to_string(),
        card_types: vec![CardType::Creature],
        subtypes: vec![CreatureType::Inkling.into()],
        colors: vec![Color::White, Color::Black],
        power: 1,
        toughness: 1,
        keywords: vec![Keyword::Flying],
        counters: vec![],
    }
}

/// The "Fractal" token (CR 111.3): a 0/0 green-and-blue Fractal, optionally entering with `counters`
/// +1/+1 counters. Shared by every Fractal-maker (Additive Evolution, Wild Hypothesis, Snarl Song, …).
pub(crate) fn fractal_token(counters: u32) -> TokenSpec {
    TokenSpec {
        name: "Fractal".to_string(),
        card_types: vec![CardType::Creature],
        subtypes: vec![CreatureType::Fractal.into()],
        colors: vec![Color::Green, Color::Blue],
        power: 0,
        toughness: 0,
        keywords: vec![],
        counters: if counters > 0 {
            vec![(CounterKind::PlusOnePlusOne, counters)]
        } else {
            vec![]
        },
    }
}

/// The "Elemental" token (CR 111.3): a 3/3 blue-and-red Elemental with flying. Shared by every
/// Elemental-maker (Muse's Encouragement, Artistic Process, Visionary's Dance, …).
pub(crate) fn elemental_token() -> TokenSpec {
    TokenSpec {
        name: "Elemental".to_string(),
        card_types: vec![CardType::Creature],
        subtypes: vec![CreatureType::Elemental.into()],
        colors: vec![Color::Blue, Color::Red],
        power: 3,
        toughness: 3,
        keywords: vec![Keyword::Flying],
        counters: vec![],
    }
}

/// The "Spirit" token (CR 111.3): a 2/2 red-and-white Spirit. Shared by every Spirit-maker (Group
/// Project, Living History, Garrison Excavator, …). Ready for the next Spirit-maker card.
#[allow(dead_code)]
pub(crate) fn spirit_token() -> TokenSpec {
    TokenSpec {
        name: "Spirit".to_string(),
        card_types: vec![CardType::Creature],
        subtypes: vec![CreatureType::Spirit.into()],
        colors: vec![Color::Red, Color::White],
        power: 2,
        toughness: 2,
        keywords: vec![],
        counters: vec![],
    }
}

/// "Earthbend N" (CR 611 animation + counters) — the chosen **target land you control** becomes a
/// 0/0 creature with haste that's still a land, gets `n` +1/+1 counters, and (engine-side) gains the
/// "when it dies or is exiled, return it tapped" delayed trigger. Shared by every earthbender
/// (Badgermole Cub's ETB earthbend 1, Earthbender Ascension's ETB earthbend 2, Ba Sing Se's
/// activated earthbend 2). The target is always "target land you control" — even on ETB triggers
/// (the engine enumerates it at 601.2c/603.3d via `collect_specs_into`).
pub(crate) fn earthbend(n: i64) -> Effect {
    Effect::Earthbend {
        target: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Permanent(land_you_control()),
            min: 1,
            max: 1,
            distinct: true,
        }),
        n: ValueExpr::Fixed(n),
    }
}

/// As [`fetch_basic_tapped`] but searched by `who` — e.g. Erode, where the *destroyed* permanent's
/// controller (`ControllerOfTarget(0)`) "may search" (the `min: 0` is the "may": they pick 0 or 1).
pub(crate) fn fetch_basic_tapped_by(who: PlayerRef) -> Effect {
    Effect::Search {
        who,
        zone: Zone::Library,
        filter: basic_land_filter(),
        min: 0,
        max: 1,
        to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
        tapped: true,
    }
}
