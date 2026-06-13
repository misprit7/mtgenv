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
use crate::protocol::{ClientMsg, DeckCardView, DeckEntry, ServerMsg};
use crate::session::{ClientResponse, GreSessionAgent};
use mtg_core::priority::StopConfig;
use mtg_core::state::GameState;

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

/// Snapshot a seat's **starting decklist** from the freshly-built `GameState` (before the engine
/// draws opening hands), grouped by card with counts. This is for the human's debug zone viewer
/// only — it is read straight from `GameState`, never via `PlayerView`, so it can't leak into the
/// agent boundary. Library *order* is discarded (grouped), so nothing about draw order is exposed.
fn decklist_for(state: &GameState, seat: PlayerId) -> Vec<DeckEntry> {
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
                    subtypes: c.subtypes.clone(),
                    supertypes: c.supertypes.clone(),
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
fn stops_msg(s: &StopConfig) -> ServerMsg {
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
