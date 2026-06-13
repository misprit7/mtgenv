//! Cross-cutting MTG vocabulary primitives — the foundational colors, zones, phases, status,
//! mana, counters, damage, and reference types shared by the agent boundary (`agent`),
//! the Effect IR (`effects`), and the engine's `state`/`turn`/`mana`/`chars` modules, so there
//! is **one canonical home** for each (CLAUDE.md: one import path per item). Import from
//! `crate::basics::*` rather than redefining.
//!
//! CR anchors: colors (105), zones (400), status (110.5), mana (106), counters (122),
//! damage (120), turn structure (500s).

use crate::ids::{ObjId, PlayerId, StackId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The five colors + colorless (CR 105). Colorless is not a color, but it's convenient to
/// carry it here for mana accounting; use `Option<Color>`/the `Colorless` arm as appropriate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Color {
    White,
    Blue,
    Black,
    Red,
    Green,
    Colorless,
}

impl Color {
    /// The five true colors (excludes `Colorless`), in WUBRG order.
    pub const WUBRG: [Color; 5] = [
        Color::White,
        Color::Blue,
        Color::Black,
        Color::Red,
        Color::Green,
    ];
}

/// Card types (CR 300s). The shared-vocabulary home for the type both the effect IR filters on
/// and the engine's state/characteristics carry. (Supertypes/subtypes are strings on objects.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CardType {
    Artifact,
    Battle,
    Creature,
    Enchantment,
    Instant,
    Land,
    Planeswalker,
    Sorcery,
    Kindred,
}

impl CardType {
    /// The canonical type-line word for this card type.
    pub fn as_str(self) -> &'static str {
        match self {
            CardType::Artifact => "Artifact",
            CardType::Battle => "Battle",
            CardType::Creature => "Creature",
            CardType::Enchantment => "Enchantment",
            CardType::Instant => "Instant",
            CardType::Land => "Land",
            CardType::Planeswalker => "Planeswalker",
            CardType::Sorcery => "Sorcery",
            CardType::Kindred => "Kindred",
        }
    }

    /// Whether a card of this type becomes a permanent on the battlefield (CR 110.4).
    /// Instants and sorceries never do (400.4a); Kindred only ever appears alongside a
    /// permanent type, so on its own it is not a permanent type.
    pub fn is_permanent(self) -> bool {
        matches!(
            self,
            CardType::Artifact
                | CardType::Battle
                | CardType::Creature
                | CardType::Enchantment
                | CardType::Land
                | CardType::Planeswalker
        )
    }
}

/// The seven zones (CR 400). Public vs. hidden is a property of the zone + viewer, enforced by
/// the `PlayerView` masking function — not encoded here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Zone {
    Library,
    Hand,
    Battlefield,
    Graveyard,
    Stack,
    Exile,
    Command,
}

/// Phases and steps of a turn, flattened (CR 500s). The engine's `turn` machine drives these;
/// the agent's `PlayerView` names the current one. (If `turn` later wants a richer phase+step
/// split it can re-home this; the agent only needs to name the beat.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Phase {
    Untap,
    Upkeep,
    Draw,
    PrecombatMain,
    BeginCombat,
    DeclareAttackers,
    DeclareBlockers,
    CombatDamage,
    EndCombat,
    PostcombatMain,
    End,
    Cleanup,
}

/// The four independent status booleans every permanent has (CR 110.5). Defaults on entry:
/// untapped, unflipped, face up, phased in (110.5b) — i.e. all `false`, the derived `Default`.
/// Status exists only on the battlefield.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Status {
    pub tapped: bool,
    pub flipped: bool,
    pub face_down: bool,
    pub phased_out: bool,
}

/// A mana cost (CR 106/202): generic + per-color requirements. `BTreeMap` keeps it
/// deterministic for hashing/serialization/snapshotting. (Hybrid/Phyrexian/X live in the
/// cost vocabulary in `effects`, not here — this is the plain colored+generic core.)
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManaCost {
    pub generic: u32,
    pub colored: BTreeMap<Color, u32>,
    /// The number of `{X}` symbols in the printed cost (CR 107.3) — usually 0, 1 for `{X}`-spells,
    /// 2 for `{X}{X}`. The chosen value of X is picked at cast (`ChooseNumber`) and lives in the
    /// resolution context, not here.
    #[serde(default)]
    pub x: u32,
}

/// A mana pool (CR 106.4): mana currently available to a player, by color. Empties as each
/// step/phase ends (CR 500.5).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManaPool {
    pub amounts: BTreeMap<Color, u32>,
}

impl ManaPool {
    /// Total mana of all colors currently in the pool.
    pub fn total(&self) -> u32 {
        self.amounts.values().copied().sum()
    }
}

/// A kind of counter (CR 122). The common built-ins are named; `Keyword`/`Named` are escape
/// hatches so the set is open without churn. `+1/+1` and `-1/-1` are distinct kinds (they
/// annihilate as an SBA, CR 704.5q).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CounterKind {
    PlusOnePlusOne,
    MinusOneMinusOne,
    Loyalty,
    Poison,
    Charge,
    Shield,
    Stun,
    Finality,
    Defense,
    /// A keyword-granting counter (CR 122.1b), e.g. a "flying" counter.
    Keyword(String),
    /// Any other named counter.
    Named(String),
}

/// A multiset of counters on an object or player (`kind → count`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CounterBag {
    pub counts: BTreeMap<CounterKind, u32>,
}

impl CounterBag {
    pub fn get(&self, kind: &CounterKind) -> u32 {
        self.counts.get(kind).copied().unwrap_or(0)
    }
}

/// How damage is being dealt (CR 120). Affects which triggers/replacements apply (e.g.
/// "combat damage", lifelink, deathtouch handling all key off this + source characteristics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageKind {
    Combat,
    Noncombat,
}

/// A position within a zone an object can be placed at (CR 401 ordering; library top/bottom).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ZonePos {
    Top,
    Bottom,
    /// Nth from the top (0 = top).
    Nth(u32),
    /// Order/position is irrelevant (e.g. entering the battlefield, a hand, a graveyard).
    Any,
}

/// A zone + position destination (for `MoveZone`, scry/surveil staging, search results).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ZoneDest {
    pub zone: Zone,
    pub pos: ZonePos,
}

/// A reference to a thing an effect/ability can act on or target: a player, a game object
/// (permanent, or a card in a public zone), or an object on the stack (a spell or ability).
/// This is the *concrete* reference; selection *criteria* live in `effects::target`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Target {
    Player(PlayerId),
    Object(ObjId),
    Stack(StackId),
}
