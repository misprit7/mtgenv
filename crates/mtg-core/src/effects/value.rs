//! Dynamic values and player references used throughout the Effect IR. A `ValueExpr` is a
//! number that may read game state at resolution time (CR 608.2h — info read once, at
//! application); a `PlayerRef` names a player relative to the effect's source/controller.

use super::target::CardFilter;
use crate::basics::{CounterKind, Zone};
use serde::{Deserialize, Serialize};

/// A player named relative to the resolving effect. Resolved against the `ResolutionCtx` when
/// an `Action` is materialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlayerRef {
    /// The controller of the effect's source (the usual "you").
    Controller,
    /// The single opponent (2-player). Generalizes to "each opponent" via `EachOpponent`.
    Opponent,
    EachOpponent,
    EachPlayer,
    /// The owner of the effect's source.
    Owner,
    /// A player chosen as the Nth target of this effect.
    ChosenTarget(u32),
    /// The *controller* of the Nth (object) target of this effect, snapshotted at resolution
    /// start — so it survives that object leaving play during the same resolution (e.g. Erode's
    /// "Destroy target creature. Its controller may search…", where "its controller" is read
    /// before the destroy moves it to the graveyard). CR 608.2 (info read as the effect resolves).
    ControllerOfTarget(u32),
}

/// A number that may be fixed or computed from game state. Kept small; grows with the IR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueExpr {
    /// A literal amount.
    Fixed(i64),
    /// The value of X chosen at cast/activation (CR 107.3).
    X,
    /// A fixed multiple of X (e.g. "twice X").
    XTimes(i64),
    /// The number of targets this effect has.
    NumTargets,
    /// Count objects in a zone matching a filter, optionally restricted by controller.
    Count {
        zone: Zone,
        filter: CardFilter,
        controller: Option<PlayerRef>,
    },
    /// Sum of `a` and `b` (composition so simple arithmetic is expressible without new nodes).
    Sum(Box<ValueExpr>, Box<ValueExpr>),
    /// The number of counters of `kind` on **this object** — the resolving effect's source at
    /// resolution time, or the object being computed in a layer-7a CDA (`SetBasePTValue`). Used
    /// for "P/T = the number of +1/+1 counters on it" and "double the counters on this" effects.
    CountersOnSelf(CounterKind),
    /// The **computed power** of the Nth chosen target, snapshotted at resolution (CR 608.2h). For
    /// "double its power" (Mightform): `PumpPT{ power: PowerOfTarget(0), toughness: Fixed(0) }`
    /// adds the target's current power to itself. `0` if the target isn't an object on the
    /// battlefield.
    PowerOfTarget(u32),
    /// The total mana spent to cast the effect's source object (CR 601.2f–h, incl. any `{X}`),
    /// recorded as it was cast and read when it enters. For "enters with +1/+1 counters equal to
    /// the mana spent to cast it" (Dyadrine). `0` if the source wasn't cast (token / put onto the
    /// battlefield).
    ManaSpent,
}

impl ValueExpr {
    /// Convenience: a literal.
    pub fn lit(n: i64) -> Self {
        ValueExpr::Fixed(n)
    }
}
