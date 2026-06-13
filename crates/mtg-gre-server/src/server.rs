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
use axum::extract::Query;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use mtg_core::agent::{Agent, RandomAgent};
use mtg_core::basics::Phase;
use mtg_core::ids::PlayerId;
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

use crate::driver;
use crate::protocol::{ClientMsg, ServerMsg};
use crate::session::{ClientResponse, GreSessionAgent};

/// The self-contained, no-build client served when `web/dist/` is absent.
const EMBEDDED_CLIENT: &str = include_str!("embedded_client.html");

/// Batch-resolved Scryfall art manifest (grp_id → art_crop/normal/artist). Generated once by
/// the resolver script and baked in, so the client never queries the Scryfall API at runtime —
/// it only loads the images from Scryfall's CDN (cached). Regenerate when the card pool grows.
const CARD_ART: &str = include_str!("../card-art.json");

/// A per-connection seed, so successive games vary while staying replayable.
static SEED: AtomicU64 = AtomicU64::new(1);

/// Build the axum app: a `/ws` endpoint plus static serving of the front end.
pub fn app() -> Router {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("web/dist");
    let mut router = Router::new()
        .route("/ws", get(ws_handler))
        .route("/card-art.json", get(card_art));
    if dist.join("index.html").exists() {
        // Built Vite front end available — serve it, falling back to the embedded client only
        // for unmatched routes.
        router = router.fallback_service(ServeDir::new(dist).fallback(get(embedded)));
    } else {
        router = router.fallback(get(embedded));
    }
    router
}

/// Serve the embedded no-build client.
async fn embedded() -> impl IntoResponse {
    Html(EMBEDDED_CLIENT)
}

/// Serve the baked-in Scryfall art manifest (grp_id → image URLs + artist).
async fn card_art() -> impl IntoResponse {
    ([(axum::http::header::CONTENT_TYPE, "application/json")], CARD_ART)
}

/// Build the `Stops` echo (current live stop config) the UI renders the phase bar/toggles from.
fn stops_msg(s: &driver::Stops) -> ServerMsg {
    ServerMsg::Stops {
        auto_pass: s.auto_pass,
        full_control: s.full_control,
        smart_stops: s.smart_stops,
        resolve_own_stack: s.resolve_own_stack,
        per_step: s.effective_steps(),
    }
}

/// Bind `addr` and serve until the process exits.
pub async fn serve(addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let local = listener.local_addr()?;
    println!("mtg-gre-server listening on http://{local}  (open it in a browser to play)");
    axum::serve(listener, app()).await
}

/// `/ws?p0=<deck>&p1=<deck>` — deck names (`burn`/`bears`/`demo`) pick each seat's deck; unset =
/// demo. `?autopass=0` plays paper-CR (prompt every window); `?fullcontrol=1` stops everywhere.
/// Seat 0 is the human (browser), seat 1 the `RandomAgent`.
async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let p0 = params.get("p0").cloned();
    let p1 = params.get("p1").cloned();
    let truthy = |v: &str| v == "1" || v.eq_ignore_ascii_case("on") || v.eq_ignore_ascii_case("true");
    let flag = |key: &str, dflt: bool| params.get(key).map(|v| truthy(v)).unwrap_or(dflt);
    // `?stops=PrecombatMain:1,Upkeep:0` — per-step stop overrides (Phase names = serde variants).
    let overrides: Vec<(Phase, bool)> = params
        .get("stops")
        .map(|s| {
            s.split(',')
                .filter_map(|tok| {
                    let (name, val) = tok.split_once(':')?;
                    let phase: Phase = serde_json::from_str(&format!("\"{name}\"")).ok()?;
                    Some((phase, val != "0"))
                })
                .collect()
        })
        .unwrap_or_default();
    let stops = driver::Stops {
        // MTGA defaults for human play; ?autopass=0 opts into every-window prompting.
        auto_pass: flag("autopass", true),
        full_control: flag("fullcontrol", false),
        smart_stops: flag("smartstops", true),
        resolve_own_stack: flag("resolvestack", true),
        overrides,
    };
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
    let (to_client_tx, mut to_client_rx) = tokio::sync::mpsc::unbounded_channel::<ServerMsg>();
    let (from_client_tx, from_client_rx) = std::sync::mpsc::channel::<ClientResponse>();

    // The stop config is SHARED + live-mutable: the socket task mutates it on SetStop/SetOption,
    // the GreSessionAgent reads it on every decision → mid-game stop toggling with NO reset.
    let stops_handle = Arc::new(Mutex::new(stops));
    let echo_tx = to_client_tx.clone(); // socket task → client (stop-config echoes)
    let result_tx = to_client_tx.clone(); // game thread → client (final GameOver frame)
    let agent_stops = stops_handle.clone();
    std::thread::spawn(move || {
        let human = GreSessionAgent::new(PlayerId(0), to_client_tx, from_client_rx, agent_stops);
        let bot = RandomAgent::new(seed);
        let agents: Vec<Box<dyn Agent>> = vec![Box::new(human), Box::new(bot)];
        // Decks chosen by the client (default demo = lands + creatures + burn), so the browser
        // game exercises casting & combat — and the user can pick e.g. Burn vs Bears.
        let state = driver::state_for_decks(p0.as_deref(), p1.as_deref(), seed);
        // Engine auto-pass stays OFF (prompts every window); the GreSessionAgent applies the
        // human's live stop policy itself, so the human can re-stop steps mid-game.
        let outcome = driver::run_state(state, agents);
        let _ = result_tx.send(ServerMsg::GameOver {
            winner: outcome.winner,
        });
    });

    // Echo the initial stop config so the phase bar / toggles render the live state.
    let _ = echo_tx.send(stops_msg(&stops_handle.lock().unwrap()));

    let (mut sink, mut stream) = socket.split();

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
                            // Live stop changes: mutate the shared config + echo it back. The
                            // running game's GreSessionAgent picks it up at the next window.
                            Ok(ClientMsg::SetStop { step, on }) => {
                                {
                                    let mut s = stops_handle.lock().unwrap();
                                    s.overrides.retain(|(p, _)| *p != step);
                                    s.overrides.push((step, on));
                                }
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
