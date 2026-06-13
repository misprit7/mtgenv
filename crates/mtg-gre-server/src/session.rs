//! M2 — [`GreSessionAgent`]: the same [`Agent`] boundary bridged over a WebSocket.
//!
//! It is the in-process sibling of MTGA's GRE seam (CLIENT_PLAN §2): `decide()` projects the
//! engine's [`DecisionRequest`] into a [`Prompt`](crate::options::Prompt), ships it down the
//! socket, and **blocks the game thread** until the matching response arrives — then maps the
//! selection back into a [`DecisionResponse`]. `observe()` pushes state events.
//!
//! Concurrency (CLIENT_PLAN §6.1): `mtg-core` is synchronous, so each game runs on its own
//! thread; the bridge talks to the async WebSocket task over two channels — an unbounded tokio
//! channel for server→client pushes, and a std mpsc for client→server responses (blocking
//! `recv` on the game thread). All async is confined to [`crate::server`].

use std::sync::mpsc::Receiver;

use mtg_core::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView, RandomAgent};
use mtg_core::ids::PlayerId;
use tokio::sync::mpsc::UnboundedSender;

use crate::options::{self, Selection};
use crate::protocol::ServerMsg;

/// A client's answer to one prompt (the decoded inbound `response` message).
#[derive(Debug, Clone)]
pub struct ClientResponse {
    pub id: u64,
    pub picks: Vec<u32>,
    pub number: Option<i64>,
    pub pass: bool,
    pub order: Vec<u32>,
}

/// An [`Agent`] whose decisions are answered by a remote WebSocket client.
pub struct GreSessionAgent {
    seat: PlayerId,
    to_client: UnboundedSender<ServerMsg>,
    from_client: Receiver<ClientResponse>,
    next_id: u64,
    /// Fallback used if the client disconnects mid-game so the game thread terminates cleanly
    /// instead of hanging (the engine never sees a missing answer).
    fallback: RandomAgent,
}

impl GreSessionAgent {
    pub fn new(
        seat: PlayerId,
        to_client: UnboundedSender<ServerMsg>,
        from_client: Receiver<ClientResponse>,
    ) -> Self {
        GreSessionAgent {
            seat,
            to_client,
            from_client,
            next_id: 1,
            fallback: RandomAgent::new(0xC0FFEE ^ seat.0 as u64),
        }
    }
}

impl Agent for GreSessionAgent {
    fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
        let id = self.next_id;
        self.next_id += 1;
        let prompt = options::prompt_for(view, req);
        // If the client is already gone, don't even try the round-trip.
        if self
            .to_client
            .send(ServerMsg::Decide {
                id,
                prompt,
                view: view.clone(),
            })
            .is_err()
        {
            return self.fallback.decide(view, req);
        }
        // Block the game thread until the matching response arrives (ignore stale ids).
        loop {
            match self.from_client.recv() {
                Ok(r) if r.id == id => {
                    let sel = Selection {
                        picks: r.picks,
                        number: r.number,
                        pass: r.pass,
                        order: r.order,
                    };
                    return options::response_from(req, &sel);
                }
                Ok(_) => continue,
                // Client disconnected → fall back so the game finishes and the thread exits.
                Err(_) => return self.fallback.decide(view, req),
            }
        }
    }

    fn observe(&mut self, view: &PlayerView, ev: &GameEvent) {
        // Best-effort push; a closed socket just means nobody's listening.
        let _ = self.to_client.send(ServerMsg::Event {
            event: ev.clone(),
            view: view.clone(),
        });
    }
}

impl GreSessionAgent {
    /// This seat's id (used by the server when wiring a match).
    pub fn seat(&self) -> PlayerId {
        self.seat
    }
}
