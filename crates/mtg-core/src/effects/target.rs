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
    /// "target player" / "target opponent" / "target you" (CR 115.1) — the candidate set is
    /// restricted by the `PlayerFilter` relative to the source's controller.
    Player(PlayerFilter),
    /// A permanent matching the filter.
    Permanent(CardFilter),
    /// A creature (sugar for `Permanent` of creatures).
    Creature(CardFilter),
    /// A spell or ability on the stack.
    StackObject(CardFilter),
    /// A card in a public zone (graveyard/exile).
    CardInZone { zone: Zone, filter: CardFilter },
}

/// Which players a "target player" word accepts, relative to the source's controller (CR 115.1 /
/// 109.5). Kept small and extensible — a new restriction (e.g. "a player who controls a creature")
/// becomes a new variant, not a special case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerFilter {
    /// "target player" — any player (including yourself).
    Any,
    /// "target opponent" — any player other than the source's controller (CR 102.1).
    Opponent,
    /// "you" as a target (rare; kept for completeness).
    You,
}

/// One "target" requirement (one instance of the word "target"): its kind and how many.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetSpec {
    pub kind: TargetKind,
    pub min: u32,
    /// The maximum number of targets. The sentinel [`TARGET_COUNT_X`] means "up to X" — resolved to
    /// the spell's chosen `{X}` at cast-time slot construction (CR 601.2b/c; the value, not the pip
    /// count). Only the cast slot-builders interpret it; it's never read at resolution re-validation.
    pub max: u32,
    /// If true, the targets must be distinct objects (the common case).
    pub distinct: bool,
}

/// `TargetSpec.max` sentinel: "up to X target …" — the maximum is the spell's chosen `{X}` (Divergent
/// Equation's "return up to X target instant and/or sorcery cards"). Resolved to `chosen_x` where the
/// cast slot-builder has it in scope; `u32::MAX` is never a real printed maximum, so it's unambiguous.
pub const TARGET_COUNT_X: u32 = u32::MAX;

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
    /// Has a printed/granted keyword ability (CR 702), read from COMPUTED chars — e.g. "target
    /// creature with flying" (Glorious Decay). Evaluated via `has_keyword`.
    HasKeyword(Keyword),
    /// Matches a supertype on the object (CR 205.4) — `Basic`, `Legendary`, `Snow`, …
    /// e.g. a basic land = `All([HasCardType(Land), Supertype(Supertype::Basic)])`.
    Supertype(Supertype),
    HasColor(Color),
    Colorless,
    /// Two or more colors (CR 105.4b) — "a multicolored spell" (Mage Tower Referee). Reads computed
    /// colors, so a color-adding effect flows through.
    Multicolored,
    /// Mana value within `[min, max]` (inclusive); `None` = unbounded.
    ManaValue { min: Option<u32>, max: Option<u32> },
    /// Mana value within a **dynamic** `[min, max]` (inclusive), each bound an evaluated
    /// [`ValueExpr`] (`None` = unbounded) — for filters keyed to a resolution value, e.g. "mana
    /// value X" (Fix What's Broken: `{min: Some(X), max: Some(X)}`) or "mana value X or less"
    /// (Vicious Rivalry: `{max: Some(X)}`). Resolved to a concrete [`ManaValue`] against the
    /// resolution context (`resolve_dynamic_filter`) before matching, so ctx-free matchers only
    /// ever see the concrete form.
    ManaValueExpr { min: Option<Box<ValueExpr>>, max: Option<Box<ValueExpr>> },
    /// Computed power at most `n` (CR — "creature with power 2 or less"). Escape Tunnel.
    PowerAtMost(i32),
    /// Computed toughness at most `n` (CR — "with toughness 1 or less"). Pairs with `PowerAtMost`
    /// under `AnyOf` for "power or toughness N or less" (Arnyn's dies-trigger filter).
    ToughnessAtMost(i32),
    /// Computed power at least `n` (CR — "creature with power 4 or greater"). Pairs with
    /// `ToughnessAtLeast` under `AnyOf` for "power or toughness N or greater" (Repel Calamity).
    PowerAtLeast(i32),
    /// Computed toughness at least `n` (CR — "creature with toughness 4 or greater").
    ToughnessAtLeast(i32),
    /// Controlled by the named player.
    ControlledBy(PlayerRef),
    Tapped,
    Untapped,
    /// A creature that is **currently attacking** (declared as an attacker this combat, CR 508.1) —
    /// e.g. Living History's "target attacking creature." Matches iff the object is in
    /// `GameState.combat.attackers`.
    Attacking,
    /// Has a counter of this kind.
    HasCounter(CounterKind),
    /// Matches a specific named card (rare; for the few effects that name a card).
    Named(String),
    /// Matches iff the candidate is a **stack object with exactly one target** (CR 115.7 — Return the
    /// Favor's "target spell or ability with a single target"). Read directly off the stack object's
    /// chosen `targets` (not the underlying card's characteristics), so it's only meaningful for a
    /// `TargetKind::StackObject`; on a non-stack candidate it never matches (fail-closed).
    HasSingleTarget,
    /// Matches iff the object's name equals the **chosen name noted on the matcher's SOURCE** (its
    /// [`crate::state::Object::chosen_name`]) — Petrified Hamlet's "Lands with the chosen name have
    /// '{T}: Add {C}'". A dynamic sibling of [`Named`] whose target isn't fixed at authoring; the
    /// grant's static reads the source Hamlet's runtime choice. No match while the source has no
    /// chosen name.
    NamedAsChooser,
    /// Matches iff the object's **owner** (CR 108.3) is the named player — "a spell you don't own" =
    /// `Not(OwnedBy(Controller))` (Nita, Forum Conciliator's cast trigger). Distinct from
    /// [`ControlledBy`], which reads the controller. `PlayerRef` is resolved relative to the matcher's
    /// perspective (for a cast trigger, `Controller` = the caster).
    OwnedBy(PlayerRef),
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

/// A spend restriction on produced mana (CR 106.6) — "spend this mana only to …". Carried on the
/// `ManaSpec` so the payment path can gate where the mana is usable. One variant for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpendRestriction {
    /// "Spend this mana only to cast instant and sorcery spells." (SoS: Hydro-Channeler, Abstract
    /// Paintmage, Great Hall of the Biblioplex.)
    InstantSorceryOnly,
}

/// Mana an ability/effect produces (CR 605/106). A simple bag; one entry per produced color.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManaSpec {
    /// Produced amounts keyed by color (use `Color::Colorless` for `{C}`).
    pub produces: Vec<(Color, ValueExpr)>,
    /// "Any one color"-style production: the controller chooses the color when it resolves.
    pub any_color: Option<ValueExpr>,
    /// "Add {B} or {G} for each …" (CR 106.1c) — produce `count` mana where **each** mana is
    /// independently one of `colors` (the controller chooses each mana's colour as it's produced).
    /// Distinct from [`any_color`](Self::any_color) (N mana, all of one of the five colours) and
    /// [`produces`](Self::produces) (fixed colours). Empty `colors` reads as all five (defensive).
    /// `#[serde(default)]` so existing serialized data round-trips. (Culling Ritual: `([B, G], N)`.)
    #[serde(default)]
    pub one_of: Option<(Vec<Color>, ValueExpr)>,
    /// A spend restriction on the produced mana (CR 106.6, e.g. "only to cast instant and sorcery
    /// spells"). `None` = unrestricted. `#[serde(default)]` so existing serialized data round-trips.
    #[serde(default)]
    pub restriction: Option<SpendRestriction>,
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
