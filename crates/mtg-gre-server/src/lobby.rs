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
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot};

use mtg_core::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView, RandomAgent};
use mtg_core::ids::PlayerId;
use mtg_core::priority::StopConfig;

use crate::driver;
use crate::protocol::ServerMsg;
use crate::session::{ClientResponse, GreSessionAgent};

// ── Spectating: a per-room fan-out of the seat-0 view stream ──────────────────────────────────

/// A live broadcast of a game's frames to any number of read-only spectators, plus a cache of the
/// latest board frame so a spectator who joins mid-game sees the current state immediately.
struct SpectateHub {
    tx: broadcast::Sender<ServerMsg>,
    last_view: Mutex<Option<ServerMsg>>,
}

impl SpectateHub {
    fn new() -> Arc<Self> {
        let (tx, _) = broadcast::channel(256);
        Arc::new(SpectateHub {
            tx,
            last_view: Mutex::new(None),
        })
    }
    /// Publish an omniscient god-view frame (the engine's replay-sink feed): cache it for late
    /// joiners and fan it out live. Spectators see no hidden information (every zone face-up).
    fn publish_god(&self, frame: &mtg_core::replay::ReplayFrame) {
        let msg = ServerMsg::GodFrame {
            state: frame.state.clone(),
            label: frame.label.clone(),
        };
        *self.last_view.lock().unwrap() = Some(msg.clone());
        let _ = self.tx.send(msg);
    }
    /// Publish a non-board frame (e.g. `GameOver`) live without disturbing the cached board.
    fn publish(&self, frame: ServerMsg) {
        let _ = self.tx.send(frame);
    }
}

/// Wraps a non-human agent and **sleeps before each decision** so a spectator can follow the game
/// at a watchable pace (the engine is single-threaded, so this paces the whole game).
struct DelayAgent {
    inner: Box<dyn Agent>,
    delay: Duration,
}

impl Agent for DelayAgent {
    fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
        std::thread::sleep(self.delay);
        self.inner.decide(view, req)
    }
    fn observe(&mut self, view: &PlayerView, ev: &GameEvent) {
        self.inner.observe(view, ev);
    }
}

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
    /// Per-decision delay (ms) applied to **non-human** seats, so spectators can follow along.
    pub delay_ms: u32,
    start: Mutex<StartState>,
    spectate: Arc<SpectateHub>,
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
            delay_ms: self.delay_ms,
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
            // Seed game ids ABOVE any saved replay. A restart wipes the in-memory registry, so
            // without this ids would reset to 1 and new games' replay files would overwrite older
            // ones (replay filename = game id). See `server::max_replay_id`.
            next_id: AtomicU64::new(crate::server::max_replay_id() + 1),
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
    delay_ms: u32,
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
    /// Per-decision delay (ms) for non-human seats (spectator pacing); 0 = no delay.
    #[serde(default)]
    delay_ms: u32,
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

/// `DELETE /api/games/:id` — remove a game from the lobby listing. If it's mid-game, the engine
/// thread keeps its own `Arc<Room>` clone and finishes harmlessly; this only drops the listing.
pub async fn delete_game(State(lobby): State<Arc<Lobby>>, Path(id): Path<u64>) -> StatusCode {
    if lobby.rooms.lock().unwrap().remove(&id).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

/// `POST /api/games` — create a game from a seat config. Returns `{ "id": <u64> }`. Agent-only games
/// (no human seats) start running immediately.
pub async fn create_game(State(lobby): State<Arc<Lobby>>, Json(req): Json<CreateReq>) -> Response {
    if req.seats.len() < 2 || req.seats.len() > 4 {
        return (StatusCode::BAD_REQUEST, "a game needs 2-4 seats").into_response();
    }
    // Reject a typo'd/unknown deck up front (preset or custom) rather than silently falling back to
    // the demo deck at game start.
    for s in &req.seats {
        if driver::resolve_deck(&s.deck).is_none() {
            return (StatusCode::BAD_REQUEST, format!("unknown deck '{}'", s.deck)).into_response();
        }
    }
    let id = lobby.next_id.fetch_add(1, Ordering::Relaxed);
    let name = req.name.unwrap_or_else(|| format!("Game #{id}"));
    let nseats = req.seats.len();
    let room = Arc::new(Room {
        id,
        name,
        seats: req.seats,
        delay_ms: req.delay_ms.min(10_000), // clamp so a typo can't wedge a game for minutes
        start: Mutex::new(StartState {
            slots: (0..nseats).map(|_| None).collect(),
            status: Status::Waiting,
            spawned: false,
        }),
        spectate: SpectateHub::new(),
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

/// Serialize a frame and push it onto a spectator's socket. Returns `Err` if the socket is gone.
async fn send_json(sink: &mut SplitSink<WebSocket, Message>, msg: &ServerMsg) -> Result<(), ()> {
    match serde_json::to_string(msg) {
        Ok(txt) => sink.send(Message::Text(txt)).await.map_err(|_| ()),
        Err(_) => Ok(()),
    }
}

/// Handle `GET /ws?game=<id>&spectate=1`: a **read-only** viewer of a game (seat-0 perspective).
/// Subscribes to the room's broadcast, immediately replays the latest board frame (so a viewer who
/// joins mid-game isn't blank), then forwards live frames until the game ends or the viewer leaves.
/// All inbound frames are ignored — a spectator never controls anything.
pub async fn handle_spectator_socket(socket: WebSocket, lobby: Arc<Lobby>, game: u64) {
    let room = lobby.rooms.lock().unwrap().get(&game).cloned();
    let Some(room) = room else {
        return reject(socket, "no such game").await;
    };
    // Subscribe BEFORE snapshotting the cache so no live frame slips through the gap (a duplicate
    // replayed frame is harmless — it just re-renders the same view).
    let mut rx = room.spectate.tx.subscribe();
    let last = room.spectate.last_view.lock().unwrap().clone();
    let final_winner = match room.start.lock().unwrap().status {
        Status::Finished { winner } => Some(winner),
        _ => None,
    };

    let (mut sink, mut stream) = socket.split();
    // Prime the viewer with the current board (+ a GameOver if it already ended).
    if let Some(frame) = last {
        if send_json(&mut sink, &frame).await.is_err() {
            return;
        }
    }
    if let Some(winner) = final_winner {
        let _ = send_json(&mut sink, &ServerMsg::GameOver { winner: winner.map(PlayerId) }).await;
        return;
    }
    // Forward live frames; stop on GameOver or disconnect.
    loop {
        tokio::select! {
            r = rx.recv() => match r {
                Ok(frame) => {
                    let over = matches!(frame, ServerMsg::GameOver { .. });
                    if send_json(&mut sink, &frame).await.is_err() { break; }
                    if over { break; }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue, // viewer fell behind → skip ahead
                Err(broadcast::error::RecvError::Closed) => break,
            },
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {} // ignore inbound (read-only)
            }
        }
    }
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

        let delay = Duration::from_millis(room.delay_ms as u64);
        for (i, spec) in room.seats.iter().enumerate() {
            let pid = PlayerId(i as u32);
            // Spectators watch via the engine's god-view replay sink (installed below), not by
            // teeing a seat's PlayerView — so they see no hidden information.
            let agent: Box<dyn Agent> = match spec.kind {
                SeatKind::Human => {
                    let ing = slots[i].take().expect("human seat ingredients present");
                    senders.push((pid, ing.to_client_tx.clone()));
                    stop_txs.push((pid, ing.stop_handle_tx));
                    humans.push((pid, ing.stops));
                    Box::new(GreSessionAgent::new(pid, ing.to_client_tx, ing.from_client_rx))
                }
                // Rl is stubbed to RandomAgent for now (no trained agent yet). Non-human seats get
                // the spectator-pacing delay (humans pace themselves).
                SeatKind::Random | SeatKind::Rl => {
                    let base: Box<dyn Agent> = Box::new(RandomAgent::new(seed ^ (i as u64 + 1)));
                    if delay.is_zero() {
                        base
                    } else {
                        Box::new(DelayAgent { inner: base, delay })
                    }
                }
            };
            agents.push(agent);
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
        let (mut engine, handles) = driver::room_engine(state, agents, &humans);
        let handle_map: HashMap<u32, Arc<Mutex<StopConfig>>> =
            handles.into_iter().map(|(p, h)| (p.0, h)).collect();
        for (pid, stop_tx) in stop_txs {
            if let Some(h) = handle_map.get(&pid.0) {
                let _ = stop_tx.send(Arc::clone(h));
            }
        }

        // Stream omniscient (god-view) frames to spectators live: the engine's replay sink fires
        // per public event on this game thread, so we forward each god frame to the room's hub
        // (cached for late joiners). Installing the sink turns recording on, so the same frames
        // also accumulate for the on-finish auto-save (`replay()`).
        let spectate = Arc::clone(&room.spectate);
        engine.set_replay_sink(Box::new(move |frame| spectate.publish_god(frame)));

        // Play it out (recording an omniscient replay), then broadcast the result + persist the
        // replay so the lobby's finished-game "▶ Replay" button can play it back.
        let (outcome, mut replay) = driver::finish_game_with_replay(engine);
        for (i, seat) in room.seats.iter().enumerate() {
            if let Some(rp) = replay.meta.players.get_mut(i) {
                rp.name = format!("P{i} ({})", seat_kind_label(seat.kind));
                rp.deck = seat.deck.clone();
            }
        }
        replay.meta.source = mtg_core::replay::ReplaySource::Human; // a lobby game (human/AI mix)
        replay.meta.created_at = now_millis();
        crate::server::save_replay(room.id, &replay);

        for (_pid, tx) in &senders {
            let _ = tx.send(ServerMsg::GameOver {
                winner: outcome.winner,
            });
        }
        room.spectate.publish(ServerMsg::GameOver {
            winner: outcome.winner,
        });
        room.start.lock().unwrap().status = Status::Finished {
            winner: outcome.winner.map(|p| p.0),
        };
    });
}

/// A short label for a seat's controller, stamped into replay metadata.
fn seat_kind_label(kind: SeatKind) -> &'static str {
    match kind {
        SeatKind::Human => "Human",
        SeatKind::Random => "Agent",
        SeatKind::Rl => "RL",
    }
}

/// Unix epoch milliseconds (server-stamped — the core never reads a clock).
fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
