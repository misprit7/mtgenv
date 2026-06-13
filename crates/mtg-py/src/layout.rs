//! `layout` — the **single source of truth shared by the observation encoder ([`crate::obs`]) and
//! the action codec ([`crate::codec`])**: padded table sizes, the stable row ordering of each
//! entity table, and the categorical feature vocabularies.
//!
//! Why shared: a factored action slot like `PERM[i]` must point at the *same* battlefield object
//! the policy saw at observation row `i` (GYM_PLAN §4.2 — "slots = positional indices into the
//! padded observation"). So the row ordering and the table sizes live in exactly one place and both
//! sides import them. Change a size or the ordering here and obs + codec move together.

use mtg_core::agent::ObjView;
use mtg_core::basics::Color;
use mtg_core::ids::ObjId;

// ── padded table sizes (config; grow with the pool) ─────────────────────────────────────────
pub const MAX_PERM: usize = 32;
pub const MAX_HAND: usize = 16;
pub const MAX_STACK: usize = 8;

// ── categorical vocabularies (stable order; APPEND-ONLY — changing order changes the obs) ────
/// Card-type one-hot basis (must match `CardType::as_str`). `Kindred` is rare; folded out for now.
pub const CARD_TYPES: [&str; 8] = [
    "Creature",
    "Land",
    "Artifact",
    "Enchantment",
    "Planeswalker",
    "Instant",
    "Sorcery",
    "Battle",
];
/// Color one-hot basis (WUBRG).
pub const COLORS: [Color; 5] = [
    Color::White,
    Color::Blue,
    Color::Black,
    Color::Red,
    Color::Green,
];
/// Keyword bitmask basis (must match `format!("{Keyword:?}")` — the Debug variant names the view
/// emits in `CharacteristicsView.keywords`).
pub const KEYWORDS: [&str; 15] = [
    "Deathtouch",
    "Defender",
    "DoubleStrike",
    "FirstStrike",
    "Flash",
    "Flying",
    "Haste",
    "Hexproof",
    "Indestructible",
    "Lifelink",
    "Menace",
    "Reach",
    "Trample",
    "Vigilance",
    "Ward",
];

pub const N_CARD_TYPES: usize = CARD_TYPES.len();
pub const N_COLORS: usize = COLORS.len();
pub const N_KEYWORDS: usize = KEYWORDS.len();

// ── stable entity ordering + row lookup (the obs↔action contract) ───────────────────────────

/// The id of any perceived object (both `Visible` and `Hidden` carry one). The padded row order of
/// each table is simply "the first `MAX_*` of the corresponding `view` list" — both [`crate::obs`]
/// (which iterates the list) and [`crate::codec`] (which `position`s into the same `take(MAX_*)`
/// id vector) rely on exactly this, which is what keeps obs row `i` and action slot `i` aligned.
pub fn objview_id(o: &ObjView) -> ObjId {
    match o {
        ObjView::Visible { id, .. } => *id,
        ObjView::Hidden { id, .. } => *id,
    }
}
