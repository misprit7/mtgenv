//! The `Native` escape hatch (WHITEBOARD_MODEL.md §2.3): a genuinely-unique card supplies
//! hand-written Rust — the MTGA equivalent of "I gave up and wrote the CLIPS by hand." This
//! guarantees no card is ever *impossible*, only *not-yet-done in pure IR*.
//!
//! The core stays card-agnostic: it invokes a `NativeFn` through the `EffectCtx` trait and
//! never knows which card it is. A native effect may only mutate state by staging `Action`s on
//! the current whiteboard (and, later, by asking the active agent through the same decision
//! boundary) — it cannot reach around the engine.

use super::action::Action;
use crate::error::EngineError;
use crate::ids::{ObjId, PlayerId};

/// The controlled context a `Native` effect runs against. The effect runtime implements this;
/// the trait is intentionally minimal now and grows as native cards need more (reading state,
/// requesting decisions via `DecisionRequest`). Kept as a trait so `effects` does not depend on
/// the concrete engine state types.
pub trait EffectCtx {
    /// Stage an action onto the current whiteboard (the only way a native effect mutates).
    fn push_action(&mut self, action: Action);

    /// The controller of the resolving effect's source ("you").
    fn controller(&self) -> PlayerId;

    /// The effect's source object, if it still exists (LKI applies if it has left — CR 608.2h).
    fn source(&self) -> Option<ObjId>;

    /// The value chosen for X at cast/activation, if any (CR 107.3).
    fn x(&self) -> Option<u32>;
}

/// A hand-authored effect. Receives the controlled context, stages actions, returns `Ok` or an
/// `EngineError`. A plain function pointer so the enclosing `Effect`/`Ability` stays `Copy`-able
/// where possible and avoids boxed closures on the hot path.
pub type NativeFn = fn(&mut dyn EffectCtx) -> Result<(), EngineError>;
