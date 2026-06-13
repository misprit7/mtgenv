//! The **lobby**: a server-side registry of games that sits *above* the rules client. It lets the
//! UI configure *both* sides of a match — each seat is a human, a dumb test agent (`RandomAgent`),
//! or a (future) RL AI — then create / join / list games.
//!
//! A [`Room`] is one game configuration. Human seats are filled by WebSocket connections
//! (`/ws?game=<id>&seat=<n>`); when every human seat has connected, the game **auto-starts** (one
//! `std::thread` running the synchronous engine, exactly like the legacy single-game path). Agent-
//! only games run immediately on creation.
//!
//! ## Why "ingredients", not agents
//! `mtg_core::agent::Agent` has no `Send` bound, so `Box<dyn Agent>` is **not `Send`** and cannot
//! cross a `std::thread::spawn` boundary. The legacy path sidesteps this by building agents *inside*
//! the thread. We do the same: a room collects only the **`Send` ingredients** per human seat
//! ([`SeatIngredients`] — channel endpoints + stops), and the spawned game thread builds the
//! `GreSessionAgent`/`RandomAgent`s itself.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use mtg_core::agent::{Agent, RandomAgent};
use mtg_core::ids::PlayerId;
use mtg_core::priority::StopConfig;

use crate::driver;
use crate::protocol::ServerMsg;
use crate::session::{ClientResponse, GreSessionAgent};

// ── Public configuration types (serde — the wire shape of the REST API) ──────────────────────

/// Who plays a seat. `Rl` is accepted but currently **stubbed to `RandomAgent`** (no trained agent
/// exists yet); the UI labels it honestly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SeatKind {
    Human,
    Random,
    Rl,
}

/// One seat's configuration in a [`Room`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSeat {
    pub kind: SeatKind,
    /// Preset deck name (`"demo"`/`"burn"`/`"bears"`); unknown → demo.
    #[serde(default = "default_deck")]
    pub deck: String,
}

fn default_deck() -> String {
    "demo".to_string()
}

/// A game's lifecycle, surfaced to the lobby UI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum Status {
    /// Waiting for human seats to connect.
    Waiting,
    /// All seats present; the engine thread is running.
    Running,
    /// The game ended.
    Finished { winner: Option<u32> },
}

// ── Internal room state ──────────────────────────────────────────────────────────────────────

/// The `Send` channel endpoints a connected human seat contributes to its game. The spawned engine
/// thread drains these and builds the seat's `GreSessionAgent` itself (see module docs).
struct SeatIngredients {
    /// Engine → this seat's socket (events/decides/decklist/gameover). The agent moves one clone;
    /// the socket task keeps the receiver + an echo clone.
    to_client_tx: mpsc::UnboundedSender<ServerMsg>,
    /// This seat's socket → its agent (decision responses).
    from_client_rx: std::sync::mpsc::Receiver<ClientResponse>,
    /// Engine thread → socket task: the seat's live stop handle, once the engine is built.
    stop_handle_tx: oneshot::Sender<Arc<Mutex<StopConfig>>>,
    /// The seat's MTGA-style stop config.
    stops: driver::Stops,
}

/// Everything guarded by the room's single mutex: the per-seat ingredient slots (human seats fill
/// in over time), the lifecycle status, and a one-shot "already spawned" latch. Keeping all three
/// under one lock makes the "last human connector spawns the game" rendezvous race-free.
struct StartState {
    slots: Vec<Option<SeatIngredients>>,
    status: Status,
    spawned: bool,
}

/// One game configuration in the lobby.
pub struct Room {
    pub id: u64,
    pub name: String,
    pub seats: Vec<RoomSeat>,
    start: Mutex<StartState>,
}

impl Room {
    fn summary(&self) -> GameSummary {
        let st = self.start.lock().unwrap();
        let started = !matches!(st.status, Status::Waiting);
        let seats = self
            .seats
            .iter()
            .enumerate()
            .map(|(i, s)| SeatSummary {
                kind: s.kind,
                deck: s.deck.clone(),
                human: s.kind == SeatKind::Human,
                // agent seats are always "filled"; human seats are filled once their slot is
                // occupied (or the game has started, after which slots are drained).
                filled: s.kind != SeatKind::Human
                    || started
                    || st.slots.get(i).and_then(|o| o.as_ref()).is_some(),
            })
            .collect();
        GameSummary {
            id: self.id,
            name: self.name.clone(),
            seats,
            status: st.status.clone(),
        }
    }
}

/// The shared lobby (axum app state). Holds every room + id/seed counters.
pub struct Lobby {
    rooms: Mutex<HashMap<u64, Arc<Room>>>,
    next_id: AtomicU64,
    seed: AtomicU64,
}

impl Lobby {
    pub fn new() -> Arc<Self> {
        Arc::new(Lobby {
            rooms: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            seed: AtomicU64::new(1),
        })
    }
}

// ── REST DTOs ────────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct GameSummary {
    id: u64,
    name: String,
    seats: Vec<SeatSummary>,
    status: Status,
}

#[derive(Serialize)]
pub struct SeatSummary {
    kind: SeatKind,
    deck: String,
    human: bool,
    filled: bool,
}

#[derive(Deserialize)]
pub struct CreateReq {
    #[serde(default)]
    name: Option<String>,
    seats: Vec<RoomSeat>,
}

// ── REST handlers ──────────────────────────────────────────────────────────────────────────────

/// `GET /api/games` — list every game (newest ids last), with seat config + status.
pub async fn list_games(State(lobby): State<Arc<Lobby>>) -> Json<Vec<GameSummary>> {
    let rooms = lobby.rooms.lock().unwrap();
    let mut out: Vec<GameSummary> = rooms.values().map(|r| r.summary()).collect();
    out.sort_by_key(|g| g.id);
    Json(out)
}

/// `GET /api/games/:id` — one game's details (for the game page header).
pub async fn game_detail(State(lobby): State<Arc<Lobby>>, Path(id): Path<u64>) -> Response {
    match lobby.rooms.lock().unwrap().get(&id) {
        Some(r) => Json(r.summary()).into_response(),
        None => (StatusCode::NOT_FOUND, "no such game").into_response(),
    }
}

/// `POST /api/games` — create a game from a seat config. Returns `{ "id": <u64> }`. Agent-only games
/// (no human seats) start running immediately.
pub async fn create_game(State(lobby): State<Arc<Lobby>>, Json(req): Json<CreateReq>) -> Response {
    if req.seats.len() < 2 || req.seats.len() > 4 {
        return (StatusCode::BAD_REQUEST, "a game needs 2-4 seats").into_response();
    }
    let id = lobby.next_id.fetch_add(1, Ordering::Relaxed);
    let name = req.name.unwrap_or_else(|| format!("Game #{id}"));
    let nseats = req.seats.len();
    let room = Arc::new(Room {
        id,
        name,
        seats: req.seats,
        start: Mutex::new(StartState {
            slots: (0..nseats).map(|_| None).collect(),
            status: Status::Waiting,
            spawned: false,
        }),
    });
    lobby.rooms.lock().unwrap().insert(id, Arc::clone(&room));
    // No human seats → nothing to wait for; run it now (agent-vs-agent).
    try_start(&lobby, &room);
    Json(serde_json::json!({ "id": id })).into_response()
}

// ── WebSocket join (the rendezvous) ────────────────────────────────────────────────────────────

/// Politely refuse a join: tell the client why (a `log` frame the UI shows), then close.
async fn reject(mut socket: WebSocket, why: &str) {
    let msg = ServerMsg::Log {
        text: format!("join failed: {why}"),
    };
    if let Ok(txt) = serde_json::to_string(&msg) {
        let _ = socket.send(Message::Text(txt)).await;
    }
    let _ = socket.send(Message::Close(None)).await;
}

/// Handle `GET /ws?game=<id>&seat=<n>`: claim human seat `n` of game `id`. Registers this socket's
/// channel ingredients; if it completes the room's human seats, spawns the game thread. Then waits
/// for the engine to start (its stop handle) and runs the normal player socket loop.
pub async fn handle_lobby_socket(
    socket: WebSocket,
    lobby: Arc<Lobby>,
    game: u64,
    seat: usize,
    stops: driver::Stops,
) {
    // (Look the room up and drop the map guard before any await — guards aren't Send.)
    let room = lobby.rooms.lock().unwrap().get(&game).cloned();
    let Some(room) = room else {
        return reject(socket, "no such game").await;
    };
    if seat >= room.seats.len() || room.seats[seat].kind != SeatKind::Human {
        return reject(socket, "that seat is not an open human seat").await;
    }

    // Channels for this seat (mirrors the legacy single-game path).
    let (to_client_tx, to_client_rx) = mpsc::unbounded_channel::<ServerMsg>();
    let (from_client_tx, from_client_rx) = std::sync::mpsc::channel::<ClientResponse>();
    let (stop_tx, mut stop_rx) = oneshot::channel::<Arc<Mutex<StopConfig>>>();
    let echo_tx = to_client_tx.clone();

    // Claim the seat under the single room lock; compute any rejection, drop the guard, THEN await.
    let rejection: Option<&str> = {
        let mut st = room.start.lock().unwrap();
        if st.spawned || !matches!(st.status, Status::Waiting) {
            Some("the game has already started")
        } else if st.slots[seat].is_some() {
            Some("that seat is already taken")
        } else {
            st.slots[seat] = Some(SeatIngredients {
                to_client_tx,
                from_client_rx,
                stop_handle_tx: stop_tx,
                stops,
            });
            None
        }
    };
    if let Some(why) = rejection {
        return reject(socket, why).await;
    }
    // If this filled the last human seat, the game starts here (else another seat will start it).
    try_start(&lobby, &room);

    // Wait for the game to start (our stop handle arrives) OR a pre-start disconnect.
    let (sink, mut stream) = socket.split();
    let handle = loop {
        tokio::select! {
            h = &mut stop_rx => break h.ok(),
            incoming = stream.next() => match incoming {
                // Closed/errored before the game started → vacate our slot below.
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break None,
                _ => continue, // ignore any stray pre-start frames
            }
        }
    };
    let Some(handle) = handle else {
        // Pre-start disconnect: free the slot so the seat can be re-claimed and the room isn't
        // orphaned at "partially filled" forever.
        let mut st = room.start.lock().unwrap();
        if !st.spawned {
            if let Some(slot) = st.slots.get_mut(seat) {
                *slot = None;
            }
        }
        return;
    };

    // Game is live: echo the initial stops, then run the shared player loop.
    let _ = echo_tx.send(crate::server::stops_msg(&handle.lock().unwrap()));
    crate::server::run_player_socket(sink, stream, to_client_rx, from_client_tx, echo_tx, handle).await;
}

// ── Game start ─────────────────────────────────────────────────────────────────────────────────

/// Start the room's game **iff** every human seat is connected and it hasn't started yet. Idempotent
/// (drains the slots and latches `spawned`, so only one caller ever spawns).
fn try_start(lobby: &Lobby, room: &Arc<Room>) {
    let slots = {
        let mut st = room.start.lock().unwrap();
        if st.spawned {
            return;
        }
        let ready = room
            .seats
            .iter()
            .enumerate()
            .all(|(i, s)| s.kind != SeatKind::Human || st.slots[i].is_some());
        if !ready {
            return;
        }
        st.spawned = true;
        st.status = Status::Running;
        std::mem::take(&mut st.slots) // drained; only this caller now owns the ingredients
    };
    let seed = lobby.seed.fetch_add(1, Ordering::Relaxed);
    spawn_game(seed, Arc::clone(room), slots);
}

/// The game thread: builds the per-seat agents (humans from their ingredients, agent seats fresh),
/// runs the engine to completion, hands each human its live stop handle + starting decklist, and
/// broadcasts the final result.
fn spawn_game(seed: u64, room: Arc<Room>, mut slots: Vec<Option<SeatIngredients>>) {
    std::thread::spawn(move || {
        let mut agents: Vec<Box<dyn Agent>> = Vec::with_capacity(room.seats.len());
        let mut humans: Vec<(PlayerId, driver::Stops)> = Vec::new();
        let mut senders: Vec<(PlayerId, mpsc::UnboundedSender<ServerMsg>)> = Vec::new();
        let mut stop_txs: Vec<(PlayerId, oneshot::Sender<Arc<Mutex<StopConfig>>>)> = Vec::new();

        for (i, spec) in room.seats.iter().enumerate() {
            let pid = PlayerId(i as u32);
            match spec.kind {
                SeatKind::Human => {
                    let ing = slots[i].take().expect("human seat ingredients present");
                    senders.push((pid, ing.to_client_tx.clone()));
                    stop_txs.push((pid, ing.stop_handle_tx));
                    humans.push((pid, ing.stops));
                    agents.push(Box::new(GreSessionAgent::new(pid, ing.to_client_tx, ing.from_client_rx)));
                }
                // Rl is stubbed to RandomAgent for now (no trained agent yet).
                SeatKind::Random | SeatKind::Rl => {
                    agents.push(Box::new(RandomAgent::new(seed ^ (i as u64 + 1))));
                }
            }
        }

        // Build the state from each seat's deck; snapshot each human's starting decklist BEFORE the
        // engine consumes the state + draws opening hands (RL-safe — read from GameState, not view).
        let deck_refs: Vec<&str> = room.seats.iter().map(|s| s.deck.as_str()).collect();
        let state = driver::state_for_deck_names(seed, &deck_refs);
        for (pid, tx) in &senders {
            let _ = tx.send(ServerMsg::Decklist {
                seat: *pid,
                cards: crate::server::decklist_for(&state, *pid),
            });
        }

        // Build the engine with each human's stops applied; hand each seat its live stop handle.
        let (engine, handles) = driver::room_engine(state, agents, &humans);
        let handle_map: HashMap<u32, Arc<Mutex<StopConfig>>> =
            handles.into_iter().map(|(p, h)| (p.0, h)).collect();
        for (pid, stop_tx) in stop_txs {
            if let Some(h) = handle_map.get(&pid.0) {
                let _ = stop_tx.send(Arc::clone(h));
            }
        }

        // Play it out, then broadcast the result and record it.
        let outcome = driver::finish_game(engine);
        for (_pid, tx) in &senders {
            let _ = tx.send(ServerMsg::GameOver {
                winner: outcome.winner,
            });
        }
        room.start.lock().unwrap().status = Status::Finished {
            winner: outcome.winner.map(|p| p.0),
        };
    });
}
