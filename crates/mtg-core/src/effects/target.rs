//! Target/selection criteria and object "spec" types for the Effect IR. These are the
//! *criteria* (predicates) the engine resolves into concrete `basics::Target`s / object sets
//! when it builds a `DecisionRequest` (the engine enumerates the legal options — masking is
//! the engine's job, see `docs/design/AGENT_INTERFACE.md`). Distinct from `basics::Target`,
//! which is a *concrete* reference.

use super::ability::Keyword;
use super::value::{PlayerRef, ValueExpr};
use crate::basics::{CardType, Color, CounterKind, Zone};
use crate::subtypes::{Subtype, Supertype};
use serde::{Deserialize, Serialize};

/// What kind of thing a "target" word accepts (CR 115). The engine turns this + a `CardFilter`
/// into the pre-filtered legal candidate list for a `ChooseTargets` slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetKind {
    /// "any target" — creature / player / planeswalker / battle (CR 115.4).
    Any,
    Player,
    /// A permanent matching the filter.
    Permanent(CardFilter),
    /// A creature (sugar for `Permanent` of creatures).
    Creature(CardFilter),
    /// A spell or ability on the stack.
    StackObject(CardFilter),
    /// A card in a public zone (graveyard/exile).
    CardInZone { zone: Zone, filter: CardFilter },
}

/// One "target" requirement (one instance of the word "target"): its kind and how many.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetSpec {
    pub kind: TargetKind,
    pub min: u32,
    pub max: u32,
    /// If true, the targets must be distinct objects (the common case).
    pub distinct: bool,
}

/// A selection of objects an effect operates on *without* the word "target" (e.g. "sacrifice a
/// creature", "each creature you control"). Resolved at resolution time, not locked at cast.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectSpec {
    pub zone: Zone,
    pub filter: CardFilter,
    /// Who selects / whose objects (controller of source by default).
    pub chooser: PlayerRef,
    pub min: ValueExpr,
    pub max: ValueExpr,
}

/// A predicate over a card/object's characteristics (CR 109.3). A small, composable filter
/// vocabulary; `All`/`Any` compose. Serde-able (no native predicates here — use a `Native`
/// effect for genuinely uncomputable cases).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardFilter {
    /// Matches anything.
    Any,
    /// Conjunction of sub-filters.
    All(Vec<CardFilter>),
    /// Disjunction of sub-filters.
    AnyOf(Vec<CardFilter>),
    /// Negation.
    Not(Box<CardFilter>),
    HasCardType(CardType),
    HasSubtype(Subtype),
    /// Matches a supertype on the object (CR 205.4) — `Basic`, `Legendary`, `Snow`, …
    /// e.g. a basic land = `All([HasCardType(Land), Supertype(Supertype::Basic)])`.
    Supertype(Supertype),
    HasColor(Color),
    Colorless,
    /// Mana value within `[min, max]` (inclusive); `None` = unbounded.
    ManaValue { min: Option<u32>, max: Option<u32> },
    /// Computed power at most `n` (CR — "creature with power 2 or less"). Escape Tunnel.
    PowerAtMost(i32),
    /// Controlled by the named player.
    ControlledBy(PlayerRef),
    Tapped,
    Untapped,
    /// Has a counter of this kind.
    HasCounter(CounterKind),
    /// Matches a specific named card (rare; for the few effects that name a card).
    Named(String),
    /// A card with `{X}` in its mana cost (CR 107.3) — Paradox Surveyor's "a card with {X} in its
    /// mana cost". Matches when the printed cost has one or more `{X}` symbols.
    HasXInCost,
    /// Matches iff the candidate object **is the source of the ability/effect doing the
    /// matching** — i.e. "this" (CR self-referential, e.g. "prevent damage to THIS creature",
    /// "THIS enters with…"). The engine evaluates it against the matcher's source object. This
    /// is what lets self-referential replacements live in a *global* rewrite scan (which checks
    /// every permanent's replacements against an event) without leaking onto other objects.
    ItSelf,
    /// Matches iff the candidate object **is the permanent the matcher's SOURCE is attached to**
    /// (its host) — the aura/equipment analogue of `ItSelf`. Lets an aura's/equipment's buff live
    /// in the normal global static scan with no special-casing: e.g. an Equipment's "equipped
    /// creature gets +2/+0" = a static `affects` filter of `AttachedHost`. The engine
    /// resolves it via the source's `attached_to`.
    AttachedHost,
}

/// Mana an ability/effect produces (CR 605/106). A simple bag; one entry per produced color.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManaSpec {
    /// Produced amounts keyed by color (use `Color::Colorless` for `{C}`).
    pub produces: Vec<(Color, ValueExpr)>,
    /// "Any one color"-style production: the controller chooses the color when it resolves.
    pub any_color: Option<ValueExpr>,
}

/// A token's defining characteristics (CR 111.3). Used by both the `CreateToken` effect and
/// the `CreateToken` whiteboard action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSpec {
    pub name: String,
    pub card_types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub colors: Vec<Color>,
    pub power: i32,
    pub toughness: i32,
    /// Printed keyword abilities the token has (CR 111.4) — e.g. an Inkling token's Flying. Applied
    /// to the token's characteristics on creation.
    pub keywords: Vec<Keyword>,
    /// Counters the token enters with (CR 614.1e), as `(kind, count)`.
    pub counters: Vec<(CounterKind, u32)>,
    /// The `grp_id` of a registered token def (in the reserved 9000+ block) supplying this token's
    /// **triggered/activated abilities** — e.g. a Pest token's "whenever this attacks, gain 1 life".
    /// `0` = a vanilla / keyword-only token (abilities come solely from `keywords`). Stamped onto the
    /// created object's chars so `def_of` finds the abilities (CR 111.4 — tokens carry their abilities
    /// as their defining data, not name-matched by the core).
    pub grp_id: u32,
}

/// CR 707.9e "except" overrides applied to a **token that's a copy of a permanent**. The copy
/// snapshots the source's *copiable* characteristics (its base `chars`; **not** counters, damage,
/// auras, or other continuous effects — CR 707.2), then these overrides are layered on. All fields
/// empty (`Default`) = a plain copy (e.g. Colorstorm Stallion copies itself unchanged). Card-agnostic
/// data, so it lives in the Effect IR (Debug/Clone), not in serialized state.
#[derive(Debug, Clone, Default)]
pub struct TokenCopyMods {
    /// Card types the copy gains "in addition to its other types" (e.g. Applied Geometry's Creature).
    pub add_card_types: Vec<CardType>,
    /// Subtypes the copy gains (e.g. Applied Geometry's Fractal).
    pub add_subtypes: Vec<Subtype>,
    /// Overrides the copy's base power/toughness (e.g. "it's a 0/0 …"), before any counters.
    pub set_power_toughness: Option<(i32, i32)>,
    /// Counters the copy enters with, evaluated at resolution (e.g. six +1/+1 counters).
    pub counters: Vec<(CounterKind, ValueExpr)>,
}
