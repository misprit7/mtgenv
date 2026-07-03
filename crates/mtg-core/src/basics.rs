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
    /// Two-colour **hybrid** pips (CR 107.4e), each payable by *either* colour — e.g. `{B/G}` is
    /// `(Black, Green)`. Each pip counts 1 toward mana value. Monocolour hybrid (`{2/G}`) is not yet
    /// modelled. Serialized `#[serde(default)]` so older saves/wire messages load as no-hybrid.
    #[serde(default)]
    pub hybrid: Vec<(Color, Color)>,
}

impl Color {
    /// The single-letter mana symbol (CR 107.4): W/U/B/R/G, and C for colorless.
    pub fn symbol(self) -> char {
        match self {
            Color::White => 'W',
            Color::Blue => 'U',
            Color::Black => 'B',
            Color::Red => 'R',
            Color::Green => 'G',
            Color::Colorless => 'C',
        }
    }
}

impl std::fmt::Display for ManaCost {
    /// Render as MTG mana symbols (CR 107), e.g. `{2}{G}{G}` / `{X}{R}`; an empty cost is `{0}`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for _ in 0..self.x {
            write!(f, "{{X}}")?;
        }
        if self.generic > 0 {
            write!(f, "{{{}}}", self.generic)?;
        }
        for (color, n) in &self.colored {
            for _ in 0..*n {
                write!(f, "{{{}}}", color.symbol())?;
            }
        }
        if self.x == 0 && self.generic == 0 && self.colored.is_empty() {
            write!(f, "{{0}}")?;
        }
        Ok(())
    }
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

impl std::fmt::Display for CounterKind {
    /// A canonical, round-trippable string for this counter kind — used as a **map key** when a
    /// `CounterBag` serializes (JSON map keys must be strings; the derived enum repr produces a
    /// non-string for `Keyword`/`Named`, which `serde_json` rejects). Built-ins keep their Rust
    /// variant name, which is **exactly the key the derived unit-variant repr already produced** —
    /// so the existing wire format (and the web client's `CTR_LABEL` lookup) is unchanged; only the
    /// previously-crashing `Named`/`Keyword` cases gain a string form. `Named` is the bare name (so
    /// a "quest" counter shows as `quest`, not a prefixed token); `Keyword` is `kw:`-tagged so it
    /// round-trips back to the right variant. See [`CounterBag`].
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CounterKind::PlusOnePlusOne => write!(f, "PlusOnePlusOne"),
            CounterKind::MinusOneMinusOne => write!(f, "MinusOneMinusOne"),
            CounterKind::Loyalty => write!(f, "Loyalty"),
            CounterKind::Poison => write!(f, "Poison"),
            CounterKind::Charge => write!(f, "Charge"),
            CounterKind::Shield => write!(f, "Shield"),
            CounterKind::Stun => write!(f, "Stun"),
            CounterKind::Finality => write!(f, "Finality"),
            CounterKind::Defense => write!(f, "Defense"),
            CounterKind::Keyword(s) => write!(f, "kw:{s}"),
            CounterKind::Named(s) => write!(f, "{s}"),
        }
    }
}

impl std::str::FromStr for CounterKind {
    type Err = std::convert::Infallible; // total: an unrecognised string parses to `Named`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "PlusOnePlusOne" => CounterKind::PlusOnePlusOne,
            "MinusOneMinusOne" => CounterKind::MinusOneMinusOne,
            "Loyalty" => CounterKind::Loyalty,
            "Poison" => CounterKind::Poison,
            "Charge" => CounterKind::Charge,
            "Shield" => CounterKind::Shield,
            "Stun" => CounterKind::Stun,
            "Finality" => CounterKind::Finality,
            "Defense" => CounterKind::Defense,
            // A `kw:`-tagged token is a keyword counter; anything else is a free-form `Named` counter
            // (the inverse of `Display`). An unrecognised string can't fail — it's just a `Named`.
            other => match other.strip_prefix("kw:") {
                Some(k) => CounterKind::Keyword(k.to_string()),
                None => CounterKind::Named(other.to_string()),
            },
        })
    }
}

/// A multiset of counters on an object or player (`kind → count`). The `counts` map serializes with
/// **string keys** (each `CounterKind`'s canonical [`Display`]) so a JSON object is valid — the
/// derived enum repr produces non-string keys for `Keyword`/`Named`, which `serde_json` rejects
/// (this broke Replay/GodView export for quest-counter cards).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CounterBag {
    #[serde(with = "counter_map_serde")]
    pub counts: BTreeMap<CounterKind, u32>,
}

/// serde adapter: a `BTreeMap<CounterKind, u32>` as a string-keyed JSON object (CR — see [`CounterBag`]).
mod counter_map_serde {
    use super::CounterKind;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::BTreeMap;

    pub fn serialize<S: Serializer>(
        map: &BTreeMap<CounterKind, u32>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let as_strings: BTreeMap<String, u32> =
            map.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        as_strings.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<BTreeMap<CounterKind, u32>, D::Error> {
        let as_strings = BTreeMap::<String, u32>::deserialize(d)?;
        Ok(as_strings
            .into_iter()
            .map(|(k, v)| (k.parse::<CounterKind>().unwrap(), v))
            .collect())
    }
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
