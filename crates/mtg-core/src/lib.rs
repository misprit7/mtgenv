//! `mtg-core` — the headless Magic: The Gathering rules engine.
//!
//! Card-agnostic core (the "GRE") built on MTG Arena's whiteboard model. See
//! `docs/design/WHITEBOARD_MODEL.md` and `docs/plans/ENGINE_PLAN.md`.
//!
//! This is the milestone-1 scaffold: the module tree is laid out but most modules
//! are stubs. The core must never `match` on card identity — card behaviour is data
//! interpreted by the effect runtime (`effects`).

pub mod ids;
// Shared cross-cutting vocabulary + errors (owned by `design`/task #4; imported widely).
pub mod basics;
/// Closed subtype/supertype enums (CR 205.3/205.4), generated from Scryfall type catalogs by
/// `scripts/gen_subtypes.py`. Replaces stringly-typed subtypes; serde/Display keep the canonical
/// type-line string on the wire so the client view is unchanged.
pub mod subtypes;
pub mod error;
pub mod state;
pub mod chars;
pub mod conditions;
pub mod stack;
pub mod turn;
pub mod priority;
/// The resumable step API (M3) — the pull primitive `Session`/`Step`. See RESUMABLE_ENGINE.md.
pub mod session;
pub mod whiteboard;
pub mod events;
/// Replay + omniscient-spectating contract (GodView/Replay serde types). See REPLAY_PLAN.md.
pub mod replay;
pub mod sba;
pub mod combat;
pub mod mana;
pub mod cards;
pub mod rng;

// Owned by the `design` workstream (task #4): the agent/decision boundary and the
// Effect IR. Declared here as (near-)empty modules so the workspace compiles; `design`
// fills them per `docs/design/AGENT_INTERFACE.md`.
pub mod agent;
pub mod effects;
