//! Stable identities and timestamps used throughout the engine. IDs are opaque
//! newtypes (object identity, zones, stack objects, layer-system timestamps).

use serde::{Deserialize, Serialize};

/// A player / seat in the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerId(pub u32);

/// A game object's stable identity. Changing zones generally yields a NEW `ObjId`
/// (CR 400.7) — continuous effects and counters do not follow unless a rule says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ObjId(pub u64);

/// A zone's identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ZoneId(pub u32);

/// An object on the stack (a spell or an ability).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StackId(pub u64);

/// A monotonic timestamp ordering continuous effects in the layer system (CR 613.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Timestamp(pub u64);
