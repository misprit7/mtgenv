//! Engine-level error types. Kept tiny and shared so the agent boundary
//! (`agent`, decision-response validation) and the effect runtime (`effects::native`) report
//! failures through one type.

use thiserror::Error;

/// Errors raised by the engine while validating decision responses or running effects.
///
/// `IllegalResponse` is an *agent-implementation* bug, not a game event: the engine only ever
/// enumerates legal options, so a correct `Agent` cannot produce one (see
/// `docs/design/AGENT_INTERFACE.md` §4 validation contract).
#[derive(Debug, Error)]
pub enum EngineError {
    /// A `DecisionResponse` did not match the `DecisionRequest` it answered (out-of-range
    /// index, wrong count, amounts that don't sum to the required total, incomplete
    /// permutation, …).
    #[error("illegal decision response: {0}")]
    IllegalResponse(String),

    /// A `Native` effect (the escape hatch, CR-equivalent of hand-authored CLIPS) failed.
    #[error("native effect `{name}` failed: {reason}")]
    Native { name: &'static str, reason: String },

    /// A referenced object/player/stack id was not found in the current state.
    #[error("unknown reference: {0}")]
    UnknownRef(String),

    /// A generic effect-runtime error.
    #[error("effect runtime error: {0}")]
    Effect(String),
}
