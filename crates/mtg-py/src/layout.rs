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
// MAX_PERM 32→256 (contract v2, 2026-07-08): late-game SOS boards were observed at ~39 permanents,
// past the old 32-row cap — objects beyond it were silently truncated (invisible to the policy AND
// unmappable to a PERM action slot). 256 is chosen to never truncate in practice (well past any
// realistic board, degenerate grinds included); the deterministic truncation priority in
// `perm_order` remains as the safety net if that bound is ever exceeded.
pub const MAX_PERM: usize = 256;
pub const MAX_HAND: usize = 16;
pub const MAX_STACK: usize = 8;

// ── v3 contract: abstract-choice tokens + relation edges (OBS2_DESIGN.md §7) ─────────────────
/// Rows of the `choice_feat` table — content tokens for the current decision's abstract options
/// (modes / colors / numbers / yes-no). Covers every abstract bucket: MAX_MODES = MAX_NUM = 16,
/// colors 5, bool 2 — only ONE family is live per decision.
pub const MAX_CHOICE: usize = 16;
/// Rows of the `edges` tensor (relation edges, padded with −1). Bound analysis (§7.4): blocks +
/// attachments + targets + pending picks ≲ 100 on degenerate boards; overflow truncates in the
/// emission priority order (TARGETS first — see `obs::encode_edges`).
pub const MAX_EDGES: usize = 128;

/// The flat **row space** edge endpoints live in (`src_row`/`dst_row` — positions in THIS
/// observation, shared with the attention token layout; entityids never enter the tensors).
pub const ROW_BF: usize = 0; //                                    0..256   battlefield rows
pub const ROW_HAND: usize = MAX_PERM; //                         256..272   hand rows
pub const ROW_STACK: usize = MAX_PERM + MAX_HAND; //             272..280   stack rows
pub const ROW_ME: usize = ROW_STACK + MAX_STACK; //                   280   you (player token)
pub const ROW_OPP: usize = ROW_ME + 1; //                             281   opponent (player token)
pub const ROW_DECISION: usize = ROW_OPP + 1; //                       282   the current decision
pub const N_ROWS: usize = ROW_DECISION + 1;

/// Edge types (col 2 of `edges`). §7.4: pairings that need cross-row identity ONLY — unary
/// decision context (source/candidate/pending flags) stays in the feature columns.
pub const EDGE_BLOCKS: i64 = 0; //        blocker → attacker (committed + pending)
pub const EDGE_ATTACKS: i64 = 1; //       attacker → defending player
pub const EDGE_ATTACHED_TO: i64 = 2; //   aura/equipment → host
pub const EDGE_TARGETS: i64 = 3; //       stack object → entity/player (k = target slot order)
pub const EDGE_STACK_SOURCE: i64 = 4; //  ability on stack → its source permanent
pub const EDGE_PENDING_PICK: i64 = 5; //  decision → already-picked entity (k = pick order; §4a)

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

/// Is this battlefield object a land? Used only for the truncation-priority ordering in
/// [`perm_order`]. A `Hidden` permanent (face-down) has no visible types; treated as a nonland (a
/// face-down is a 2/2 creature — more decision-relevant than a land, so it keeps the higher slot).
pub fn objview_is_land(o: &ObjView) -> bool {
    match o {
        ObjView::Visible { chars, .. } => chars.card_types.iter().any(|t| t == "Land"),
        ObjView::Hidden { .. } => false,
    }
}

/// **THE permanent obs↔action contract.** The battlefield row ordering shared by the obs encoder
/// ([`crate::obs`]) and the action codec ([`crate::codec`]): returns indices into `battlefield`,
/// **capped at [`MAX_PERM`]**, partitioned nonlands-first then lands, STABLE within each class
/// (engine `view.battlefield` order preserved). Obs row `k` and codec `PERM[k]` both refer to
/// `battlefield[perm_order(battlefield)[k]]`.
///
/// When the board exceeds `MAX_PERM`, the rows dropped on overflow are the **trailing lands** — the
/// least decision-relevant permanents (a wall of tapped lands can't act; creatures/artifacts/
/// enchantments carry the choices). Both callers MUST route through this one function: a slot that
/// pointed at a different object than the policy saw would silently corrupt every combat/target
/// decision on a large board. `sort_by_key` is a stable sort, so within a class the engine's order
/// is preserved exactly (needed for reproducibility + the equivalence snapshot).
pub fn perm_order(battlefield: &[ObjView]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..battlefield.len()).collect();
    idx.sort_by_key(|&i| objview_is_land(&battlefield[i])); // false (nonland) < true (land)
    idx.truncate(MAX_PERM);
    idx
}
