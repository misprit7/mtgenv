//! The axum host (CLIENT_PLAN §6.1): serves the front end as static files and runs **one
//! lands-only game per WebSocket connection** — a human (the browser, seat 0) vs a `RandomAgent`
//! (seat 1), both behind the one [`Agent`](mtg_core::agent::Agent) boundary.
//!
//! Transport plumbing only — no rules logic. The game runs on its own thread (the engine is
//! synchronous); two channels bridge it to the async socket (see [`crate::session`]). All async
//! is confined here.
//!
//! Static serving prefers a built Vite front end at `web/dist/`; if it hasn't been built, the
//! server falls back to a self-contained embedded client so `cargo run` works with no Node step.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use mtg_core::agent::{Agent, RandomAgent};
use mtg_core::basics::Phase;
use mtg_core::ids::PlayerId;
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

use crate::driver;
use crate::protocol::{ClientMsg, DeckCardView, DeckEntry, ServerMsg};
use crate::session::{ClientResponse, GreSessionAgent};
use mtg_core::priority::StopConfig;
use mtg_core::state::GameState;

/// The self-contained, no-build **game** client served at `/play` when `web/dist/` is absent.
const EMBEDDED_CLIENT: &str = include_str!("embedded_client.html");

/// The self-contained **lobby** landing page (served at `/`). Vanilla JS, no build step — it talks
/// to the REST API (`/api/games`) and links into the game client at `/play`.
const LOBBY_HTML: &str = include_str!("lobby_client.html");

/// Batch-resolved Scryfall art manifest (grp_id → art_crop/normal/artist). Generated once by
/// the resolver script and baked in, so the client never queries the Scryfall API at runtime —
/// it only loads the images from Scryfall's CDN (cached). Regenerate when the card pool grows.
const CARD_ART: &str = include_str!("../card-art.json");

/// A per-connection seed, so successive games vary while staying replayable.
static SEED: AtomicU64 = AtomicU64::new(1);

/// Build the axum app: the lobby (`/` + `/api/games`), the game client (`/play`), the game
/// WebSocket (`/ws`), and static serving (`/assets`, art) — all sharing the [`Lobby`] state.
///
/// [`Lobby`]: crate::lobby::Lobby
pub fn app() -> Router {
    let lobby = crate::lobby::Lobby::new();
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("web/dist");
    let mut router = Router::new()
        .route("/", get(lobby_page))
        .route("/play", get(game_page))
        .route("/ws", get(ws_handler))
        .route("/card-art.json", get(card_art))
        .route(
            "/api/games",
            get(crate::lobby::list_games).post(crate::lobby::create_game),
        )
        .route(
            "/api/games/:id",
            get(crate::lobby::game_detail).delete(crate::lobby::delete_game),
        )
        .route("/api/replays", get(list_replays))
        .route("/api/replays/:id", get(get_replay));
    if dist.join("index.html").exists() {
        // Built Vite front end available — serve its /assets/* (and any stray path) via ServeDir.
        router = router.fallback_service(ServeDir::new(dist).fallback(get(embedded)));
    } else {
        router = router.fallback(get(embedded));
    }
    router.with_state(lobby)
}

/// The lobby landing page (`/`).
async fn lobby_page() -> impl IntoResponse {
    Html(LOBBY_HTML)
}

/// The game client (`/play`): the built Vite `index.html` if present, else the embedded client.
/// Read at request time so a fresh `npm run build` is picked up without restarting the server
/// (mirrors how `ServeDir` serves assets).
async fn game_page() -> impl IntoResponse {
    let idx = Path::new(env!("CARGO_MANIFEST_DIR")).join("web/dist/index.html");
    match std::fs::read_to_string(&idx) {
        Ok(html) => Html(html).into_response(),
        Err(_) => Html(EMBEDDED_CLIENT).into_response(),
    }
}

/// Serve the embedded no-build game client (also the static fallback).
async fn embedded() -> impl IntoResponse {
    Html(EMBEDDED_CLIENT)
}

/// Serve the baked-in Scryfall art manifest (grp_id → image URLs + artist).
async fn card_art() -> impl IntoResponse {
    ([(axum::http::header::CONTENT_TYPE, "application/json")], CARD_ART)
}

/// The gitignored replay store (`<repo>/data/replays`, alongside `data/scryfall/`). Computed from
/// the crate dir so it's independent of the server's working directory.
fn replay_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/replays")
}

/// Persist a finished game's [`Replay`](mtg_core::replay::Replay) to `data/replays/<id>.json`
/// (best-effort; creates the store dir). The lobby's finished-game "▶ Replay" button links to
/// `/play?replay=<id>`, so the file id matches the game id.
pub(crate) fn save_replay(id: u64, replay: &mtg_core::replay::Replay) {
    let dir = replay_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_string(replay) {
        let _ = std::fs::write(dir.join(format!("{id}.json")), json);
    }
}

/// Cheaply extract just the `meta` object from a replay file **without parsing its (multi-MB)
/// `frames`**: a replay is `{"meta":{…small…},"frames":[…huge…]}`, so we read only the first chunk
/// (meta is the first key and tiny) and deserialize the single `meta` value, stopping at its end.
/// O(chunk) per file regardless of replay size — listing stays fast as replays accumulate.
fn read_meta_prefix(path: &std::path::Path) -> Option<serde_json::Value> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; 64 * 1024];
    let n = f.read(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf[..n]);
    let key = text.find("\"meta\":")?;
    let after = &text[key + "\"meta\":".len()..];
    // Parse only the first JSON value (the meta object); trailing `,"frames":…` is ignored.
    serde_json::Deserializer::from_str(after)
        .into_iter::<serde_json::Value>()
        .next()?
        .ok()
}

/// `GET /api/replays` — list saved replays' metadata for the lobby. Replays are opaque JSON files
/// (`data/replays/*.json`, the engine's serialized `Replay`); we surface each file's `meta` fields
/// flattened, plus an `id` (filename stem). Only the small `meta` prefix of each file is read (never
/// the frames), so listing is fast even with many large replays. Missing store → `[]`.
async fn list_replays() -> impl IntoResponse {
    let mut out: Vec<serde_json::Value> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(replay_dir()) {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let (Some(stem), Some(serde_json::Value::Object(meta))) =
                (path.file_stem().and_then(|s| s.to_str()), read_meta_prefix(&path))
            else {
                continue;
            };
            let mut item = serde_json::Map::new();
            item.insert("id".into(), serde_json::Value::String(stem.to_string()));
            for (k, v) in meta {
                item.entry(k).or_insert(v);
            }
            out.push(serde_json::Value::Object(item));
        }
    }
    // Newest first by `created_at` (unix-ms), when present.
    out.sort_by_key(|v| std::cmp::Reverse(v.get("created_at").and_then(|c| c.as_i64()).unwrap_or(0)));
    axum::Json(out)
}

/// `GET /api/replays/:id` — the full replay JSON (the viewer plays its `frames`). `id` is a bare
/// filename stem, sanitized to block path traversal. 404 if absent.
async fn get_replay(
    axum::extract::Path(id): axum::extract::Path<String>,
) -> axum::response::Response {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return (axum::http::StatusCode::BAD_REQUEST, "bad replay id").into_response();
    }
    match std::fs::read_to_string(replay_dir().join(format!("{id}.json"))) {
        Ok(text) => {
            ([(axum::http::header::CONTENT_TYPE, "application/json")], text).into_response()
        }
        Err(_) => (axum::http::StatusCode::NOT_FOUND, "no such replay").into_response(),
    }
}

/// Snapshot a seat's **starting decklist** from the freshly-built `GameState` (before the engine
/// draws opening hands), grouped by card with counts. This is for the human's debug zone viewer
/// only — it is read straight from `GameState`, never via `PlayerView`, so it can't leak into the
/// agent boundary. Library *order* is discarded (grouped), so nothing about draw order is exposed.
pub(crate) fn decklist_for(state: &GameState, seat: PlayerId) -> Vec<DeckEntry> {
    use std::collections::BTreeMap;
    // grp_id → (count, representative chars). Group by printing so duplicates collapse to a count.
    let mut groups: BTreeMap<u32, (u32, DeckCardView)> = BTreeMap::new();
    for &id in &state.player(seat).library {
        let c = &state.object(id).chars;
        let mana_value = c
            .mana_cost
            .as_ref()
            .map(|m| m.generic + m.colored.values().sum::<u32>())
            .unwrap_or(0);
        let entry = groups.entry(c.grp_id).or_insert_with(|| {
            (
                0,
                DeckCardView {
                    name: c.name.clone(),
                    grp_id: c.grp_id,
                    mana_cost: c.mana_cost.clone(),
                    colors: c.colors.clone(),
                    card_types: c.card_types.clone(),
                    // Subtypes/supertypes are now enums (CR 205.3/4); render to their canonical
                    // type-line strings for this string-typed deck view (wire stays unchanged).
                    subtypes: c.subtypes.iter().map(|s| s.to_string()).collect(),
                    supertypes: c.supertypes.iter().map(|s| s.to_string()).collect(),
                    mana_value,
                },
            )
        });
        entry.0 += 1;
    }
    let mut cards: Vec<DeckEntry> = groups
        .into_values()
        .map(|(count, chars)| DeckEntry { count, chars })
        .collect();
    // Decklist order: nonland by mana value then name, lands last by name (the usual deck view).
    cards.sort_by(|a, b| {
        let land = |c: &DeckCardView| c.card_types.contains(&mtg_core::basics::CardType::Land);
        land(&a.chars)
            .cmp(&land(&b.chars))
            .then(a.chars.mana_value.cmp(&b.chars.mana_value))
            .then(a.chars.name.cmp(&b.chars.name))
    });
    cards
}

/// Build the stop-config echo (the engine's live `StopConfig` for the human seat) the UI renders
/// the phase bar / toggles from. Read straight off the shared handle the engine re-reads each window.
pub(crate) fn stops_msg(s: &StopConfig) -> ServerMsg {
    // Both turn sides of each step, zipped into one `(step, on_my_turn, on_opp_turn)` row so the
    // phase bar renders two independent dots per step. `effective_steps` yields the same ordered
    // step list for either side, so zipping them is well-defined.
    let mine = s.effective_steps(true);
    let opp = s.effective_steps(false);
    let per_step = mine
        .iter()
        .zip(opp.iter())
        .map(|(&(step, m), &(_, o))| (step, m, o))
        .collect();
    ServerMsg::Stops {
        auto_pass: s.auto_pass,
        full_control: s.full_control,
        smart_stops: s.smart_stops,
        resolve_own_stack: s.resolve_own_stack,
        per_step,
    }
}

/// Bind `addr` and serve until the process exits.
pub async fn serve(addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let local = listener.local_addr()?;
    println!("mtg-gre-server listening on http://{local}  (open it in a browser to play)");
    axum::serve(listener, app()).await
}

/// `GET /ws` — the game socket. Two shapes:
/// - **Lobby:** `?game=<id>&seat=<n>` claims human seat `n` of an existing lobby game (the room
///   auto-starts once all its human seats connect). See [`crate::lobby::handle_lobby_socket`].
/// - **Legacy/quick:** no `game` → an ephemeral one-off game, browser = seat 0 (human), seat 1 a
///   `RandomAgent`; `?p0=`/`?p1=` pick decks. Either way `?autopass=0` plays paper-CR, etc.
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(lobby): State<Arc<crate::lobby::Lobby>>,
    Query(params): Query<HashMap<String, String>>,
) -> axum::response::Response {
    let truthy = |v: &str| v == "1" || v.eq_ignore_ascii_case("on") || v.eq_ignore_ascii_case("true");
    let flag = |key: &str, dflt: bool| params.get(key).map(|v| truthy(v)).unwrap_or(dflt);
    // Defaults come from `Stops::default()` (single source of truth — SmartStops OFF + the default
    // stop set: your Main 1/2 and the opponent's Begin-Combat/End). Query params override per-flag,
    // e.g. ?autopass=0 prompts every window, ?smartstops=1 re-enables it.
    let def = driver::Stops::default();
    // `?stops=PrecombatMain:1,BeginCombat@opp:0` — per-step stop overrides layered on the defaults.
    // A bare `Name:val` sets BOTH turn sides; `Name@you:val` / `Name@opp:val` sets one side only.
    let mut overrides = def.overrides.clone();
    if let Some(s) = params.get("stops") {
        for tok in s.split(',') {
            let Some((lhs, val)) = tok.split_once(':') else { continue };
            let on = val != "0";
            let (name, side) = match lhs.split_once('@') {
                Some((n, "you")) => (n, Some(true)),
                Some((n, "opp")) => (n, Some(false)),
                _ => (lhs, None),
            };
            let Ok(phase) = serde_json::from_str::<Phase>(&format!("\"{name}\"")) else { continue };
            match side {
                Some(o) => overrides.push((phase, o, on)),
                None => overrides.extend([(phase, true, on), (phase, false, on)]),
            }
        }
    }
    let stops = driver::Stops {
        auto_pass: flag("autopass", def.auto_pass),
        full_control: flag("fullcontrol", def.full_control),
        smart_stops: flag("smartstops", def.smart_stops),
        resolve_own_stack: flag("resolvestack", def.resolve_own_stack),
        overrides,
    };
    // Lobby paths: spectate (read-only) or join a specific game's seat.
    if let Some(game) = params.get("game").and_then(|g| g.parse::<u64>().ok()) {
        if params.get("spectate").map(|v| truthy(v)).unwrap_or(false) {
            return ws.on_upgrade(move |socket| {
                crate::lobby::handle_spectator_socket(socket, lobby, game)
            });
        }
        let seat = params
            .get("seat")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        return ws.on_upgrade(move |socket| {
            crate::lobby::handle_lobby_socket(socket, lobby, game, seat, stops)
        });
    }
    // Legacy/quick path: ephemeral human-vs-RandomAgent game.
    let p0 = params.get("p0").cloned();
    let p1 = params.get("p1").cloned();
    ws.on_upgrade(move |socket| handle_socket(socket, p0, p1, stops))
}

/// One browser connection = one game. The browser is seat 0 (the human); seat 1 is a
/// `RandomAgent`. `p0`/`p1` are optional per-seat preset deck names; `stops` is the human's
/// MTGA-style auto-pass/stop config.
async fn handle_socket(
    socket: WebSocket,
    p0: Option<String>,
    p1: Option<String>,
    stops: driver::Stops,
) {
    let seed = SEED.fetch_add(1, Ordering::Relaxed);

    // server→client pushes (unbounded; sent from the sync game thread) and client→server
    // responses (std mpsc; blocking-recv on the game thread).
    let (to_client_tx, to_client_rx) = tokio::sync::mpsc::unbounded_channel::<ServerMsg>();
    let (from_client_tx, from_client_rx) = std::sync::mpsc::channel::<ClientResponse>();

    let result_tx = to_client_tx.clone(); // game thread → client (final GameOver frame)
    let deck_tx = to_client_tx.clone(); // game thread → client (starting-decklist peek)
    let echo_tx = to_client_tx.clone(); // socket task → client (stop-config echoes)
    // The engine owns the human seat's live `StopConfig`; the game thread hands its `Arc<Mutex<…>>`
    // handle back here over a oneshot so the socket task can toggle stops mid-game (the engine
    // re-reads it at the next priority window → no reset). The Engine itself never leaves the
    // thread (`dyn Agent` isn't `Send`); only the Send handle crosses.
    let (handle_tx, handle_rx) = tokio::sync::oneshot::channel::<Arc<Mutex<StopConfig>>>();
    std::thread::spawn(move || {
        let human = GreSessionAgent::new(PlayerId(0), to_client_tx, from_client_rx);
        let bot = RandomAgent::new(seed);
        let agents: Vec<Box<dyn Agent>> = vec![Box::new(human), Box::new(bot)];
        // Decks chosen by the client (default demo = lands + creatures + burn), so the browser
        // game exercises casting & combat — and the user can pick e.g. Burn vs Bears.
        let state = driver::state_for_decks(p0.as_deref(), p1.as_deref(), seed);
        // Debug library peek: snapshot the human's starting decklist from GameState (RL-safe,
        // not via PlayerView) before the engine draws opening hands, and push it to the client.
        let _ = deck_tx.send(ServerMsg::Decklist {
            seat: PlayerId(0),
            cards: decklist_for(&state, PlayerId(0)),
        });
        // Build the engine with the human's stops applied (auto-pass on by default); the engine
        // elides trivial priority windows itself and only calls the human's `decide()` at real
        // stops. Hand the live stop handle to the socket task, then play the game out.
        let (engine, handle) = driver::engine_with_stops(state, agents, PlayerId(0), &stops);
        let _ = handle_tx.send(handle);
        let outcome = driver::finish_game(engine);
        let _ = result_tx.send(ServerMsg::GameOver {
            winner: outcome.winner,
        });
    });

    // Receive the engine's live stop handle (game thread sends it before running). If the thread
    // died before sending, there's nothing to drive — bail.
    let stops_handle = match handle_rx.await {
        Ok(h) => h,
        Err(_) => return,
    };
    // Echo the initial stop config so the phase bar / toggles render the live state.
    let _ = echo_tx.send(stops_msg(&stops_handle.lock().unwrap()));

    let (sink, stream) = socket.split();
    run_player_socket(sink, stream, to_client_rx, from_client_tx, echo_tx, stops_handle).await;
}

/// Drive one human seat's WebSocket once its game is running: forward engine pushes
/// (`to_client_rx` → socket) and relay client input (socket → `from_client_tx` responses /
/// live `SetStop`/`SetOption` stop edits, echoed back). Shared by the legacy single-game path
/// ([`handle_socket`]) and the lobby room path (`crate::lobby::handle_lobby_socket`).
pub(crate) async fn run_player_socket(
    mut sink: SplitSink<WebSocket, Message>,
    mut stream: SplitStream<WebSocket>,
    mut to_client_rx: tokio::sync::mpsc::UnboundedReceiver<ServerMsg>,
    from_client_tx: std::sync::mpsc::Sender<ClientResponse>,
    echo_tx: tokio::sync::mpsc::UnboundedSender<ServerMsg>,
    stops_handle: Arc<Mutex<StopConfig>>,
) {
    // Forward server→client messages onto the socket as JSON text frames.
    let mut send_task = tokio::spawn(async move {
        while let Some(msg) = to_client_rx.recv().await {
            let txt = match serde_json::to_string(&msg) {
                Ok(t) => t,
                Err(_) => continue,
            };
            // Push the final GameOver frame too, then keep draining until the channel closes.
            if sink.send(Message::Text(txt)).await.is_err() {
                break;
            }
            if matches!(msg, ServerMsg::GameOver { .. }) {
                break;
            }
        }
    });

    // Read client responses and hand them to the game thread.
    loop {
        tokio::select! {
            incoming = stream.next() => {
                match incoming {
                    Some(Ok(Message::Text(t))) => {
                        match serde_json::from_str::<ClientMsg>(&t) {
                            Ok(ClientMsg::Response { id, picks, number, pass, order }) => {
                                // If the game thread is gone, the send just errors; we exit below.
                                if from_client_tx
                                    .send(ClientResponse { id, picks, number, pass, order })
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            // Live stop changes: mutate the engine's shared StopConfig + echo it
                            // back. The running engine re-reads it at the next priority window.
                            Ok(ClientMsg::SetStop { step, own, on }) => {
                                stops_handle.lock().unwrap().set_override(step, own, Some(on));
                                let _ = echo_tx.send(stops_msg(&stops_handle.lock().unwrap()));
                            }
                            Ok(ClientMsg::SetOption { key, on }) => {
                                {
                                    let mut s = stops_handle.lock().unwrap();
                                    match key.as_str() {
                                        "autopass" => s.auto_pass = on,
                                        "fullcontrol" => s.full_control = on,
                                        "smartstops" => s.smart_stops = on,
                                        "resolvestack" => s.resolve_own_stack = on,
                                        _ => {}
                                    }
                                }
                                let _ = echo_tx.send(stops_msg(&stops_handle.lock().unwrap()));
                            }
                            Err(_) => {}
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {} // ping/pong/binary ignored
                    Some(Err(_)) => break,
                }
            }
            _ = &mut send_task => {
                // Server side finished pushing (game over) — close the socket.
                break;
            }
        }
    }

    // Dropping from_client_tx signals the game thread to fall back and exit if still running.
    drop(from_client_tx);
    send_task.abort();
}
