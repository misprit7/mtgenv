//! Server-side **custom deck store**: user-built decks, persisted to `data/decks/<name>.json`,
//! loaded once at startup and consulted by [`crate::driver::resolve_deck`] AFTER the built-in
//! presets.
//!
//! This is a `mtg-gre-server`-only concern. The engine's `mtg_core::cards::preset_deck` (used by
//! the gym / self-play and by mtg-core itself) never sees customs — that boundary is deliberate, so
//! training stays reproducible from the card pool alone, and so a custom deck can never shadow a
//! built-in preset (presets always win a name collision, and [`save`] rejects preset names).
//!
//! Decks are stored as their canonical `(grp_id, count)` lines; [`CustomDeck::grp_ids`] expands
//! them into the flat list the engine builds a library from.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

/// One line of a custom deck: a card (by grp_id) and how many copies.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CardCount {
    pub grp_id: u32,
    pub count: u32,
}

/// A user-built deck: a display name + its card lines. Persisted verbatim as JSON.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CustomDeck {
    pub name: String,
    pub cards: Vec<CardCount>,
}

impl CustomDeck {
    /// Expand the `(grp_id, count)` lines into the flat grp_id list the engine builds a library
    /// from (order is card-major; the engine shuffles at game start anyway).
    pub fn grp_ids(&self) -> Vec<u32> {
        let mut out = Vec::new();
        for c in &self.cards {
            out.extend(std::iter::repeat(c.grp_id).take(c.count as usize));
        }
        out
    }

    /// Total number of cards.
    pub fn total(&self) -> u32 {
        self.cards.iter().map(|c| c.count).sum()
    }
}

// ── Deck-size policy ─────────────────────────────────────────────────────────────────────────────
// The engine enforces NO legality (mtg-core has no min/max/4-of/singleton checks — a deck of any
// size just builds, and a too-small one loses by decking rather than panicking). These are
// therefore server-chosen *guards*, not rules of the game:
//   * MIN_CARDS = the opening-hand size. A library smaller than this draws from an empty library on
//     the very first draw (decking out before turn 1), so the game can't meaningfully start — this
//     is the smallest total that yields a playable game.
//   * MAX_CARDS = an "absurd size" guard so a fat-fingered count can't allocate a giant library.
//   * MAX_COPIES = a per-card sanity cap (NOT the real 4-of rule — decks are intentionally
//     permissive; also prevents overflow when expanding counts).
// Partial (not-fully-implemented) cards, and small-but-legal decks, are surfaced as WARNINGS by the
// callers, never blocked here.
pub const MIN_CARDS: u32 = 7;
pub const MAX_CARDS: u32 = 300;
pub const MAX_COPIES: u32 = MAX_CARDS;
/// Max deck-name length (names double as file names).
pub const MAX_NAME_LEN: usize = 40;

fn store() -> &'static RwLock<BTreeMap<String, CustomDeck>> {
    static S: OnceLock<RwLock<BTreeMap<String, CustomDeck>>> = OnceLock::new();
    S.get_or_init(|| RwLock::new(BTreeMap::new()))
}

/// The gitignored on-disk store: `<repo>/data/decks` (alongside `data/replays`). Derived from the
/// crate dir so it's independent of the process's working directory. `data/` is already gitignored.
pub fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/decks")
}

/// The in-memory key for a name: lowercased, so lookups/uniqueness are case-insensitive (matching
/// `resolve_deck`, which lowercases preset names too).
fn key(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn file_path(name: &str) -> PathBuf {
    dir().join(format!("{}.json", key(name)))
}

/// A deck name is valid iff it's 1..=[`MAX_NAME_LEN`] chars of `[A-Za-z0-9_-]` — safe as a URL path
/// segment and as a file name, and round-trips exactly (no lossy sanitization, so the name you save
/// is the name you play). Rejected names are surfaced to the user, not silently rewritten.
pub fn valid_name(name: &str) -> bool {
    (1..=MAX_NAME_LEN).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Load every `data/decks/*.json` into the in-memory store (replacing it). Called once at startup;
/// files that don't parse (or carry an invalid name) are skipped with a stderr note rather than
/// failing the boot. A missing store dir is fine → empty store.
pub fn load_all() {
    let mut map = BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(dir()) {
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let parsed = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str::<CustomDeck>(&t).ok());
            match parsed {
                Some(deck) if valid_name(&deck.name) => {
                    map.insert(key(&deck.name), deck);
                }
                _ => eprintln!("custom decks: skipping unreadable/invalid {}", path.display()),
            }
        }
    }
    *store().write().unwrap() = map;
}

/// All custom decks (clones), sorted by (lowercased) name.
pub fn list() -> Vec<CustomDeck> {
    store().read().unwrap().values().cloned().collect()
}

/// Look up a custom deck by name (case-insensitive).
pub fn get(name: &str) -> Option<CustomDeck> {
    store().read().unwrap().get(&key(name)).cloned()
}

/// Is `name` a custom deck? (case-insensitive)
pub fn exists(name: &str) -> bool {
    store().read().unwrap().contains_key(&key(name))
}

/// Resolve a custom deck to its flat grp_id list (for [`crate::driver::resolve_deck`]). `None` if
/// there's no such custom deck.
pub fn resolve(name: &str) -> Option<Vec<u32>> {
    get(name).map(|d| d.grp_ids())
}

/// Validate + canonicalize a deck, persist it to `data/decks/<name>.json`, and update the in-memory
/// store. `overwrite` gates the mode: `false` = create (name must be new), `true` = update (name
/// must already exist). Returns the stored (canonicalized) deck, or an error message suitable as a
/// `400` body. In-memory state is only updated after the disk write succeeds.
pub fn save(mut deck: CustomDeck, overwrite: bool) -> Result<CustomDeck, String> {
    deck.name = deck.name.trim().to_string();
    if !valid_name(&deck.name) {
        return Err(format!(
            "deck name must be 1–{MAX_NAME_LEN} characters of letters, digits, '-' or '_' (no spaces)"
        ));
    }
    if crate::driver::is_preset_name(&deck.name) {
        return Err(format!("'{}' is a built-in preset — pick another name", deck.name));
    }

    // Merge duplicate grp_id lines and drop zero counts so the stored deck is canonical.
    let mut merged: BTreeMap<u32, u32> = BTreeMap::new();
    for c in &deck.cards {
        *merged.entry(c.grp_id).or_default() += c.count;
    }
    merged.retain(|_, n| *n > 0);
    if merged.is_empty() {
        return Err("deck is empty".into());
    }

    let db = mtg_core::cards::starter_db();
    for (&grp, &n) in &merged {
        if db.get(grp).is_none() {
            return Err(format!("unknown card (grp_id {grp})"));
        }
        if n > MAX_COPIES {
            return Err(format!("too many copies of grp_id {grp} ({n}; max {MAX_COPIES})"));
        }
    }
    let total: u32 = merged.values().sum();
    if total < MIN_CARDS {
        return Err(format!(
            "deck has {total} card(s); need at least {MIN_CARDS} to draw an opening hand"
        ));
    }
    if total > MAX_CARDS {
        return Err(format!("deck has {total} cards; the maximum is {MAX_CARDS}"));
    }

    let existed = exists(&deck.name);
    if overwrite && !existed {
        return Err(format!("no custom deck named '{}'", deck.name));
    }
    if !overwrite && existed {
        return Err(format!("a custom deck named '{}' already exists", deck.name));
    }

    deck.cards = merged
        .into_iter()
        .map(|(grp_id, count)| CardCount { grp_id, count })
        .collect();

    // Persist first; only touch memory if the write succeeds.
    std::fs::create_dir_all(dir()).map_err(|e| format!("cannot create deck store: {e}"))?;
    let json = serde_json::to_string_pretty(&deck).map_err(|e| e.to_string())?;
    std::fs::write(file_path(&deck.name), json).map_err(|e| format!("cannot write deck: {e}"))?;
    store().write().unwrap().insert(key(&deck.name), deck.clone());
    Ok(deck)
}

/// Delete a custom deck (memory + disk). Returns false if there was no such custom deck.
pub fn delete(name: &str) -> bool {
    if store().write().unwrap().remove(&key(name)).is_none() {
        return false;
    }
    let _ = std::fs::remove_file(file_path(name));
    true
}
