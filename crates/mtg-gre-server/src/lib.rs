//! `mtg-gre-server` — the web play interface + GRE-protocol bridge for `mtg-core`.
//!
//! Implements CLIENT_PLAN milestones 1–2 (`docs/plans/CLIENT_PLAN.md`):
//!
//! - **M1** [`human::HumanAgent`] — a stdio backend that prints the [`PlayerView`] and the
//!   enumerated legal options and reads a chosen index. Proves "a human is just another
//!   [`Agent`]".
//! - **M2** [`session::GreSessionAgent`] — the same boundary bridged over a WebSocket using a
//!   **JSON projection** of the boundary types (NOT protobuf yet — that's M3). [`server`] is the
//!   axum host that serves the TS front end and runs one game per connection.
//!
//! Everything here sits *behind* the single decision boundary (`mtg_core::agent::Agent`): the
//! engine pre-enumerates the legal options, and these backends only ever *select* among them.
//! The core stays headless — all async / IO / wire concerns live in this crate (CLIENT_PLAN §2).
//!
//! [`PlayerView`]: mtg_core::agent::PlayerView
//! [`Agent`]: mtg_core::agent::Agent

pub mod options;
pub mod protocol;
pub mod human;
pub mod session;
pub mod driver;
pub mod server;
