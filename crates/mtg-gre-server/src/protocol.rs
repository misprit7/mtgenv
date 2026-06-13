//! The **JSON projection** of the boundary types carried over the WebSocket (CLIENT_PLAN §5/§6,
//! milestone 2). This is *not* protobuf — that's M3. The boundary types already derive `serde`,
//! so the projection is mostly the boundary types verbatim plus a thin envelope:
//!
//! - server→client: a [`ServerMsg`] for each state push ([`ServerMsg::Event`]) and each decision
//!   prompt ([`ServerMsg::Decide`], carrying the flat [`Prompt`]); and
//! - client→server: a [`ClientMsg::Response`] selecting among the enumerated options.
//!
//! One outstanding decision exists at a time (the engine is single-threaded), but each prompt
//! still carries an `id` the client echoes — the JSON sibling of GRE's `msgId`→`respId`
//! correlation (CLIENT_PLAN §4.3).

use crate::options::Prompt;
use mtg_core::agent::{GameEvent, PlayerView};
use mtg_core::ids::PlayerId;
use serde::{Deserialize, Serialize};

/// Server → client. Two channels over one socket: pushes ([`Event`](ServerMsg::Event)) and
/// prompts ([`Decide`](ServerMsg::Decide)), mirroring the engine's `observe()` / `decide()`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerMsg {
    /// A public/seat-visible event, with the refreshed (information-filtered) view.
    Event {
        event: GameEvent,
        view: PlayerView,
    },
    /// A decision point: render `prompt`'s enumerated options (the masking) and reply with a
    /// [`ClientMsg::Response`] carrying the same `id`.
    Decide {
        id: u64,
        prompt: Prompt,
        view: PlayerView,
    },
    /// The game ended.
    GameOver {
        winner: Option<PlayerId>,
    },
    /// A free-text server log line (diagnostics).
    Log {
        text: String,
    },
}

/// Client → server. The only inbound message: a selection answering the current prompt.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMsg {
    Response {
        id: u64,
        #[serde(default)]
        picks: Vec<u32>,
        #[serde(default)]
        number: Option<i64>,
        #[serde(default)]
        pass: bool,
        #[serde(default)]
        order: Vec<u32>,
    },
}
