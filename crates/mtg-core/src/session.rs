//! The resumable step API (M3 — `docs/design/RESUMABLE_ENGINE.md`).
//!
//! This module hosts the **pull** primitive that inverts the engine's control flow: instead of the
//! game loop blocking inside `Agent::decide`, a `Session` runs the game to its next decision and
//! **returns** a [`Step`], then resumes when handed the response. It is the substrate for
//! GIL-free fleet stepping (many games advanced in Rust, Python seeing only batched tensors) and
//! the thin driver the blocking `Agent` trait collapses onto.
//!
//! Landing incrementally (green at every commit). M3.1 lands the [`Step`] boundary type (stable
//! regardless of the coroutine internals). M3.2 adds the coroutine-backed `Session` with
//! `resume`/`submit` over an agent-free `EngineCore`; M3.3 the fleet stepper. Until then this is
//! just the type contract.

use crate::agent::{DecisionRequest, PlayerView};
use crate::ids::PlayerId;
use crate::priority::Outcome;

/// What a [`Session`](self) yields each time it is advanced (`resume`): either the game reached a
/// player decision and is suspended awaiting a response, or the game is over.
///
/// Mirrors the `resume`/`submit` sketch pre-agreed in `GYM_PLAN.md` §2.2-B, with one addition: the
/// `Decision` variant carries the info-filtered [`PlayerView`] by value. Once suspended, the game
/// state lives inside the coroutine's stack and cannot be borrowed out, so everything a caller
/// needs at the decision point — the seat, its view (for obs encoding / the agent), and the
/// enumerated legal request — must travel in the yield. Building the view is free: `ask` already
/// computes it today.
#[derive(Debug, Clone)]
pub enum Step {
    /// The engine reached a choice point for `seat` and is suspended. Feed the chosen
    /// [`DecisionResponse`](crate::agent::DecisionResponse) back (via the session) to continue.
    Decision {
        seat: PlayerId,
        view: PlayerView,
        request: DecisionRequest,
    },
    /// The game ended (CR 104). The session is finished; `outcome` is the result.
    GameOver { outcome: Outcome },
}

impl Step {
    /// The decision's seat, if this is a `Decision`.
    pub fn seat(&self) -> Option<PlayerId> {
        match self {
            Step::Decision { seat, .. } => Some(*seat),
            Step::GameOver { .. } => None,
        }
    }
    /// Whether the game has ended.
    pub fn is_over(&self) -> bool {
        matches!(self, Step::GameOver { .. })
    }
}
