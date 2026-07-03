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
use tower_http::compression::CompressionLayer;
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
        .route("/api/replays/:id", get(get_replay))
        .route("/api/decks", get(list_decks).post(create_deck))
        .route("/api/decks/:name", get(get_deck).put(update_deck).delete(delete_deck))
        .route("/api/cards", get(card_catalog));
    if dist.join("index.html").exists() {
        // Built Vite front end available — serve its /assets/* (and any stray path) via ServeDir.
        router = router.fallback_service(ServeDir::new(dist).fallback(get(embedded)));
    } else {
        router = router.fallback(get(embedded));
    }
    // gzip/br response compression. Replay JSON (`/api/replays/:id`) is huge and massively redundant
    // (god-view board snapshot per frame) — tens of MB raw, which never finishes downloading over a
    // phone connection; it compresses ~20-30×. Transparent (negotiated via Accept-Encoding), and the
    // WS upgrade at `/ws` is unaffected (a 101 has no body to compress). Card art is external
    // (Scryfall CDN), so there's nothing already-compressed worth excluding here.
    router.layer(CompressionLayer::new()).with_state(lobby)
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

/// Cards in a playable deck that have **no usable art** in the baked-in manifest ([`CARD_ART`]) —
/// either absent entirely or present with a null `art`/`img` URL. Returns `(grp_id, name)` sorted.
///
/// The "should have art" set is [`driver::deck_card_pool`] (every card that can appear in a game);
/// a card counts as covered only if its manifest entry carries both an `art_crop` and a `normal`
/// image URL. Pure (no IO) so it's unit-testable; [`serve`] calls it once at startup to warn.
pub fn missing_card_art() -> Vec<(u32, String)> {
    // The manifest is a static include; a parse failure means everything is "missing".
    let manifest: HashMap<String, serde_json::Value> =
        serde_json::from_str(CARD_ART).unwrap_or_default();
    let has_art = |grp: u32| -> bool {
        manifest
            .get(&grp.to_string())
            .map(|e| e.get("art").is_some_and(|v| v.is_string()) && e.get("img").is_some_and(|v| v.is_string()))
            .unwrap_or(false)
    };
    driver::deck_card_pool()
        .into_iter()
        .filter(|(grp, _)| !has_art(*grp))
        .collect()
}

/// Log a warning at startup for any deck card missing art, with the one command that fixes it.
/// Art is *baked in* (the client never hits Scryfall at runtime), so a gap means a card will show
/// as a blank/text-only tile until the manifest is regenerated — surface it loudly but don't fail.
fn warn_missing_card_art() {
    let missing = missing_card_art();
    if missing.is_empty() {
        return;
    }
    eprintln!(
        "⚠ card art: {} deck card(s) have no baked-in art and will render without an image:",
        missing.len()
    );
    for (grp, name) in &missing {
        eprintln!("    • {name} (grp_id {grp})");
    }
    eprintln!(
        "  Fix: regenerate the manifest, then rebuild —\n    \
         python3 crates/mtg-gre-server/resolve-card-art.py && cargo build -p mtg-gre-server"
    );
}

/// The gitignored replay store (`<repo>/data/replays`, alongside `data/scryfall/`). Computed from
/// the crate dir so it's independent of the server's working directory.
fn replay_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/replays")
}

/// The highest numeric replay id already saved on disk (`data/replays/<n>.json`), or 0 if none.
/// The lobby seeds its game-id counter above this so a restart — which wipes the in-memory game
/// registry and would otherwise reissue ids from 1 — can't reuse an id whose replay file already
/// exists and silently overwrite it (the bug this fixes). Non-numeric stems (AI-training replays
/// like `aitrain-…`) are ignored.
pub fn max_replay_id() -> u64 {
    let mut max = 0;
    if let Ok(entries) = std::fs::read_dir(replay_dir()) {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Some(n) = path.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u64>().ok()) {
                max = max.max(n);
            }
        }
    }
    max
}

/// Persist a finished game's [`Replay`](mtg_core::replay::Replay) to `data/replays/<id>.json`
/// (best-effort; creates the store dir). The lobby's finished-game "▶ Replay" button links to
/// `/play?replay=<id>`, so the file id matches the game id.
pub(crate) fn save_replay(id: u64, replay: &mtg_core::replay::Replay) {
    let dir = replay_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    // Persist the compact (delta-encoded) form — 100×+ smaller raw than the full-frame JSON, so
    // `data/replays` stops ballooning to GBs (mtg-core `RESUMABLE`/replay notes). `get_replay`
    // reconstructs full frames on read; pre-v2 full-frame files still load via `AnyReplay`.
    if let Ok(json) = serde_json::to_string(&replay.to_compact()) {
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

/// Whether `id` is a safe replay filename stem: ASCII alphanumerics plus `-`, `_`, and `.` — the
/// last is required because **gym exporters embed dotted tokens** (a version like `2.7` and a unix-ms
/// timestamp), e.g. `aitrain-2.7-swine-…-1783094604932`. Any `..` and any path separator are
/// rejected, so `id` can only ever name a file directly inside `replay_dir()` (no traversal). This
/// is why such replays previously 400'd: the old check disallowed `.` and never reached the reader.
fn valid_replay_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains("..")
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Optional query for [`get_replay`]. `?format=compact` opts into the small v2 delta payload (the
/// web player reconstructs it client-side); anything else / absent = the default full-frame JSON.
#[derive(serde::Deserialize)]
struct ReplayQuery {
    format: Option<String>,
}

/// `GET /api/replays/:id` — the replay JSON the viewer plays. `id` is a bare filename stem,
/// sanitized to block path traversal. 404 if absent.
///
/// **Format negotiation (opt-in, no deploy coupling):** the bare endpoint always returns
/// **reconstructed full frames** (`frames[].state.…`) — the shape every existing consumer expects,
/// forever. `?format=compact` returns the **v2 compact** (delta + interned characteristics) payload
/// (~46× smaller) for a client that knows how to reconstruct it. Files are stored compact; pre-v2
/// full-frame files also load, so old saved replays keep working either way.
async fn get_replay(
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<ReplayQuery>,
) -> axum::response::Response {
    if !valid_replay_id(&id) {
        return (axum::http::StatusCode::BAD_REQUEST, "bad replay id").into_response();
    }
    let text = match std::fs::read_to_string(replay_dir().join(format!("{id}.json"))) {
        Ok(t) => t,
        Err(_) => return (axum::http::StatusCode::NOT_FOUND, "no such replay").into_response(),
    };
    let any = match serde_json::from_str::<mtg_core::replay::AnyReplay>(&text) {
        Ok(a) => a,
        Err(_) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "corrupt replay").into_response(),
    };
    let json = if q.format.as_deref() == Some("compact") {
        // Emit v2 compact: cheap when the file is already compact; convert a legacy file if needed.
        match any {
            mtg_core::replay::AnyReplay::Compact(c) => serde_json::to_string(&c),
            mtg_core::replay::AnyReplay::Legacy(r) => serde_json::to_string(&r.to_compact()),
        }
    } else {
        // Default: reconstruct full frames (unchanged behaviour for the current web player etc.).
        serde_json::to_string(&any.into_replay())
    };
    match json {
        Ok(j) => ([(axum::http::header::CONTENT_TYPE, "application/json")], j).into_response(),
        Err(_) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "replay serialize error").into_response(),
    }
}

// ── deck viewer: serve the playable presets' contents from the card DB ──────────────────────────

/// One card entry in a deck (count + the card's display characteristics, incl. the partial-card
/// flag so the lobby viewer can render the ⚠ badge). Built from `CardDef`, not a game state.
#[derive(serde::Serialize)]
struct DeckCard {
    count: u32,
    grp_id: u32,
    name: String,
    mana_cost: Option<mtg_core::basics::ManaCost>,
    mana_value: u32,
    colors: Vec<mtg_core::basics::Color>,
    card_types: Vec<String>,
    subtypes: Vec<String>,
    supertypes: Vec<String>,
    power: Option<i32>,
    toughness: Option<i32>,
    rules_text: String,
    fully_implemented: bool,
}

#[derive(serde::Serialize)]
struct DeckSummary { name: String, total: u32, partial: u32, custom: bool }

#[derive(serde::Serialize)]
struct DeckDetail { name: String, total: u32, partial: u32, custom: bool, cards: Vec<DeckCard> }

/// Project a `CardDef` (by grp_id, with a copy `count`) into the wire [`DeckCard`]. The single
/// place a card becomes JSON — shared by the deck viewer ([`build_deck`]) and the builder's card
/// catalog ([`card_catalog`]) so both speak the exact same shape.
fn card_view(grp: u32, count: u32, def: &mtg_core::cards::CardDef) -> DeckCard {
    let c = &def.chars;
    let mana_value = c
        .mana_cost
        .as_ref()
        .map(|m| m.generic + m.colored.values().sum::<u32>())
        .unwrap_or(0);
    DeckCard {
        count,
        grp_id: grp,
        name: c.name.clone(),
        mana_cost: c.mana_cost.clone(),
        mana_value,
        colors: c.colors.clone(),
        card_types: c.card_types.iter().map(|t| t.as_str().to_string()).collect(),
        subtypes: c.subtypes.iter().map(|s| s.to_string()).collect(),
        supertypes: c.supertypes.iter().map(|s| s.to_string()).collect(),
        power: c.power,
        toughness: c.toughness,
        rules_text: def.text.clone(),
        fully_implemented: def.fully_implemented,
    }
}

/// The usual deck-view order: nonland first (by mana value then name), lands last.
fn deck_sort(cards: &mut [DeckCard]) {
    cards.sort_by(|a, b| {
        let is_land = |c: &DeckCard| c.card_types.iter().any(|t| t == "Land");
        is_land(a)
            .cmp(&is_land(b))
            .then(a.mana_value.cmp(&b.mana_value))
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// Resolve a deck name to (total, partial-count, grouped+sorted card list) using `resolve_deck`
/// (presets + customs) and the engine's `starter_db` for each card's characteristics.
fn build_deck(name: &str) -> Option<(u32, u32, Vec<DeckCard>)> {
    use std::collections::BTreeMap;
    let ids = driver::resolve_deck(name)?;
    let db = mtg_core::cards::starter_db();
    let mut counts: BTreeMap<u32, u32> = BTreeMap::new();
    for &g in &ids {
        *counts.entry(g).or_default() += 1;
    }
    let mut partial = 0u32;
    let mut cards: Vec<DeckCard> = counts
        .iter()
        .filter_map(|(&g, &count)| {
            let def = db.get(g)?;
            if !def.fully_implemented {
                partial += count;
            }
            Some(card_view(g, count, def))
        })
        .collect();
    deck_sort(&mut cards);
    let total = counts.values().sum();
    Some((total, partial, cards))
}

/// `GET /api/decks` — every deck the picker offers, with card/partial counts. Built-in **presets**
/// (`custom: false`, read-only) first, then user-built **custom** decks (`custom: true`).
async fn list_decks() -> impl IntoResponse {
    let mut decks: Vec<DeckSummary> = driver::DECK_NAMES
        .iter()
        .filter_map(|&n| {
            build_deck(n).map(|(total, partial, _)| DeckSummary {
                name: n.to_string(),
                total,
                partial,
                custom: false,
            })
        })
        .collect();
    for d in crate::custom_decks::list() {
        if let Some((total, partial, _)) = build_deck(&d.name) {
            decks.push(DeckSummary { name: d.name, total, partial, custom: true });
        }
    }
    axum::Json(decks)
}

/// `GET /api/decks/:name` — a deck's full grouped card list (preset or custom) for the viewer /
/// builder-edit flow. `custom` marks whether it's an editable user deck.
async fn get_deck(axum::extract::Path(name): axum::extract::Path<String>) -> axum::response::Response {
    match build_deck(&name) {
        Some((total, partial, cards)) => {
            let custom = crate::custom_decks::exists(&name);
            axum::Json(DeckDetail { name, total, partial, custom, cards }).into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "no such deck").into_response(),
    }
}

/// `POST /api/decks` — create a new **custom** deck. Body is the deck JSON
/// (`{ "name", "cards": [{ "grp_id", "count" }] }`). Returns the deck's summary on success, or
/// `400` with a human-readable reason (unknown card, empty, too big/small, preset-name clash…).
async fn create_deck(
    axum::Json(deck): axum::Json<crate::custom_decks::CustomDeck>,
) -> axum::response::Response {
    save_deck_response(deck, false)
}

/// `PUT /api/decks/:name` — replace an existing custom deck's contents. The URL name is
/// authoritative (rename isn't supported here). `404`/`400` if it isn't an existing custom deck.
async fn update_deck(
    axum::extract::Path(name): axum::extract::Path<String>,
    axum::Json(mut deck): axum::Json<crate::custom_decks::CustomDeck>,
) -> axum::response::Response {
    deck.name = name;
    save_deck_response(deck, true)
}

/// Shared create/update tail: persist the deck (validating first) and return its summary or the
/// validation error.
fn save_deck_response(deck: crate::custom_decks::CustomDeck, overwrite: bool) -> axum::response::Response {
    match crate::custom_decks::save(deck, overwrite) {
        Ok(saved) => {
            let (total, partial) =
                build_deck(&saved.name).map(|(t, p, _)| (t, p)).unwrap_or((saved.total(), 0));
            axum::Json(DeckSummary { name: saved.name, total, partial, custom: true }).into_response()
        }
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, e).into_response(),
    }
}

/// `DELETE /api/decks/:name` — delete a custom deck. Presets are read-only (`403`); a name that
/// isn't a known custom deck is `404`.
async fn delete_deck(
    axum::extract::Path(name): axum::extract::Path<String>,
) -> axum::response::Response {
    if driver::is_preset_name(&name) {
        return (axum::http::StatusCode::FORBIDDEN, "presets are read-only").into_response();
    }
    if crate::custom_decks::delete(&name) {
        axum::http::StatusCode::NO_CONTENT.into_response()
    } else {
        (axum::http::StatusCode::NOT_FOUND, "no such custom deck").into_response()
    }
}

/// `GET /api/cards` — the deck builder's card catalog: every **deck-legal** registered card (the
/// full engine registry via `CardDb::iter`, minus tokens), each projected into the same [`DeckCard`]
/// shape as the deck viewer. `count` is meaningless here (always 1) — the builder treats these as
/// templates.
///
/// This is the whole implemented pool, not just cards used by a preset, so newly-authored cards are
/// buildable immediately. Art can lag (the manifest is regenerated separately) — a card without a
/// baked image just renders as a text tile, which the client handles gracefully.
///
/// **Tokens are excluded**: a token carries the `Token` supertype and can only be *created* by an
/// effect, never put in a deck, so it has no place in the builder. (The art dump — `dump-cards` /
/// [`driver::all_cards`] — keeps them, so a token still gets art for when it appears on the
/// battlefield.) Today no registered card is a token, so this filters nothing; it guards the day
/// predefined token cards land in the DB.
async fn card_catalog() -> impl IntoResponse {
    use mtg_core::subtypes::Supertype;
    let db = mtg_core::cards::starter_db();
    let mut cards: Vec<DeckCard> = db
        .iter()
        .filter(|(_, def)| !def.chars.supertypes.contains(&Supertype::Token))
        .map(|(grp, def)| card_view(grp, 1, def))
        .collect();
    deck_sort(&mut cards);
    axum::Json(cards)
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
        full_control: s.full_control,
        per_step,
    }
}

/// Bind `addr` and serve until the process exits.
pub async fn serve(addr: &str) -> std::io::Result<()> {
    crate::custom_decks::load_all(); // restore user-built decks from data/decks/ across restarts
    warn_missing_card_art();
    let listener = TcpListener::bind(addr).await?;
    let local = listener.local_addr()?;
    println!("mtg-gre-server listening on http://{local}  (open it in a browser to play)");
    axum::serve(listener, app()).await
}

/// `GET /ws` — the game socket. Two shapes:
/// - **Lobby:** `?game=<id>&seat=<n>` claims human seat `n` of an existing lobby game (the room
///   auto-starts once all its human seats connect). See [`crate::lobby::handle_lobby_socket`].
/// - **Legacy/quick:** no `game` → an ephemeral one-off game, browser = seat 0 (human), seat 1 a
///   `RandomAgent`; `?p0=`/`?p1=` pick decks. Either way `?fullcontrol=1` stops at every window, etc.
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(lobby): State<Arc<crate::lobby::Lobby>>,
    Query(params): Query<HashMap<String, String>>,
) -> axum::response::Response {
    let truthy = |v: &str| v == "1" || v.eq_ignore_ascii_case("on") || v.eq_ignore_ascii_case("true");
    let flag = |key: &str, dflt: bool| params.get(key).map(|v| truthy(v)).unwrap_or(dflt);
    // Defaults come from `Stops::default()` (single source of truth — Full Control OFF + the default
    // stop set: your Main 1/2 and the opponent's Begin-Combat/End). `?fullcontrol=1` prompts at every
    // window; `?stops=…` overrides individual steps.
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
        full_control: flag("fullcontrol", def.full_control),
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
                                    if key == "fullcontrol" {
                                        s.full_control = on; // the one live global stop knob
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: gym-exported replays 400'd on GET because their id embeds dotted tokens (a
    /// `2.7` version + a unix-ms timestamp) and the old sanitizer disallowed `.`. Dotted ids must
    /// pass; traversal (`..`, path separators) must still be rejected.
    #[test]
    fn replay_id_allows_dotted_gym_names_but_blocks_traversal() {
        // The exact failing file's stem, plus other real ids.
        assert!(valid_replay_id("aitrain-2.7-swine-200k-swine-step0199936-1783094604932"));
        assert!(valid_replay_id("42")); // numeric human game
        assert!(valid_replay_id("aitrain-bears-step10"));
        // Traversal / separators / empty rejected.
        assert!(!valid_replay_id(""));
        assert!(!valid_replay_id(".."));
        assert!(!valid_replay_id("../../etc/passwd"));
        assert!(!valid_replay_id("a/b"));
        assert!(!valid_replay_id("a\\b"));
        assert!(!valid_replay_id("foo..bar")); // no `..` anywhere (defense in depth)
    }

    /// Regression: a gym `AiTraining` export is the pre-v2 full-frame shape (`{meta,frames:[{state,
    /// label}]}`, no `version`); `get_replay`'s reader (`AnyReplay`) must load it. (This was never the
    /// actual 400 cause — the id was — but pin reader-tolerance so a gym file always deserializes.)
    #[test]
    fn gym_written_legacy_replay_loads_via_any_replay() {
        let json = r#"{"meta":{"players":[{"seat":0,"name":"PPO","deck":"swine"}],"result":{"winner":0,"turns":16,"reason":"ZeroLife"},"source":{"AiTraining":{"step":199936}},"created_at":1783094604932},"frames":[{"state":{"turn":1,"active_player":0,"phase":"Untap","priority_player":null,"players":[{"player":0,"life":20,"poison":0,"mana_pool":{"amounts":{}},"counters":{"counts":{}},"hand":[],"library":[],"graveyard":[],"exile":[]}],"battlefield":[],"stack":[],"combat":null},"label":"start"}]}"#;
        let any: mtg_core::replay::AnyReplay =
            serde_json::from_str(json).expect("gym legacy replay must deserialize");
        let replay = any.into_replay();
        assert_eq!(replay.frames.len(), 1);
        assert_eq!(replay.meta.result.unwrap().turns, 16);
        assert_eq!(replay.meta.source, mtg_core::replay::ReplaySource::AiTraining { step: 199936 });
        // And it re-serves in both formats (what get_replay does).
        assert!(serde_json::to_string(&replay).is_ok()); // default full
        assert!(serde_json::to_string(&replay.to_compact()).is_ok()); // ?format=compact
    }

    /// Every card that can appear in a game must have baked-in art. This is the offline half of the
    /// startup warning: if a card is added to a deck without regenerating `card-art.json`, this
    /// fails — forcing art-first (run `resolve-card-art.py` + rebuild). Guards against the silent
    /// "card renders as a blank tile" regression the user hit with the Selesnya set cards.
    #[test]
    fn every_deck_card_has_baked_in_art() {
        let missing = missing_card_art();
        assert!(
            missing.is_empty(),
            "deck cards missing baked-in art (run crates/mtg-gre-server/resolve-card-art.py then \
             rebuild): {missing:?}"
        );
    }
}
