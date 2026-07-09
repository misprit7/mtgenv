//! The milestone-1 **observation encoder** — the swappable seam from the engine's info-filtered
//! [`PlayerView`] to a structured set of fixed-shape tensors (GYM_PLAN §3).
//!
//! It reads only `PlayerView`, so hidden-information masking is inherited, not re-done (a leak is
//! structurally impossible — the encoder never sees `GameState`). Output is a [`Obs`] of:
//! - `globals` — turn, phase one-hot, active/priority flags, per-seat life/zone-counts/mana, stack
//!   depth, a decision-kind one-hot, a couple of request scalars, and two player-candidate flags
//!   (is each player a legal target of the *current* decision);
//! - `bf_feat`/`bf_ids` — one row per battlefield object (computed P/T, types/colors/keywords,
//!   status, counters, combat role, **+ two decision flags: is this object the source / a legal
//!   candidate of the current decision**, **+ a per-attacker blocked-by count and an is_pending_combat
//!   self-flag** for mid-declaration double-block/attack awareness) + its `grp_id` (the card-embedding
//!   id, separated out for an embedding lookup in the policy's features extractor);
//! - `hand_feat`/`hand_ids` — own hand rows (+ a castable flag + the same two decision flags);
//! - `stack_feat`/`stack_ids` — stack rows (+ the same two decision flags);
//! - `decision_ids` — the resolved *source card* `grp_id` driving the current decision (Tier 1):
//!   for a spell mid-cast it's the spell's id; for an ability it follows the ability back to its
//!   source permanent (whose stack row is otherwise identity-less). 0 when there's no source.
//!   Python one-hots it through the same deck-local card index as the entity tables.
//!
//! Row ordering and table sizes come from [`crate::layout`] so an action slot points at the same
//! row the policy saw. Everything here is data-only (no PyO3) so it unit-tests in pure Rust.

use mtg_core::agent::{CharacteristicsView, DecisionRequest, ObjView, PlayerPublicView, PlayerView};
use mtg_core::basics::{Color, Phase, Target};
use mtg_core::ids::{ObjId, StackId};
use std::collections::{BTreeMap, BTreeSet};

use crate::layout::{
    CARD_TYPES, COLORS, EDGE_ATTACHED_TO, EDGE_ATTACKS, EDGE_BLOCKS, EDGE_PENDING_PICK,
    EDGE_STACK_SOURCE, EDGE_TARGETS, KEYWORDS, MAX_CHOICE, MAX_EDGES, MAX_HAND, MAX_PERM,
    MAX_STACK, N_CARD_TYPES, N_COLORS, N_KEYWORDS, ROW_DECISION, ROW_HAND, ROW_ME, ROW_OPP,
    ROW_STACK,
};

const PHASES: [Phase; 12] = [
    Phase::Untap,
    Phase::Upkeep,
    Phase::Draw,
    Phase::PrecombatMain,
    Phase::BeginCombat,
    Phase::DeclareAttackers,
    Phase::DeclareBlockers,
    Phase::CombatDamage,
    Phase::EndCombat,
    Phase::PostcombatMain,
    Phase::End,
    Phase::Cleanup,
];

/// Number of `DecisionRequest` variants (the decision one-hot width).
pub const NUM_REQUESTS: usize = 21;

/// The two per-row decision flags every entity table carries (Tier 1): `is_decision_source`,
/// `is_decision_candidate` for the *current* decision.
pub const DECISION_FLAGS: usize = 2;
/// Combat linkage (Tier 2): a per-permanent "blocked-by count" = how many blockers are ganging this
/// attacker, counting both blocks committed in the engine AND blocks assigned so far in the current
/// DeclareBlockers decision (pending). This is what makes deliberate double-blocking observable — the
/// binary `blocking` flag flattens gang (≥2) vs single (1) away (CR 509). Appended, so no index shifts.
pub const COMBAT_LINK: usize = 1;
/// Combat linkage (Tier 2b): a per-permanent "is_pending_combat" flag = this creature (mine) is
/// already assigned in the CURRENT in-flight combat decision — a declared attacker mid-DeclareAttackers
/// or an assigned blocker mid-DeclareBlockers. `blocked_by` (COMBAT_LINK) gives the *attacker-side*
/// count of a forming gang; this gives the *blocker-/attacker-side* self-view — which of my own
/// creatures I've already committed — so the value/feature net (which never sees the action mask) can
/// tell a double-block is in progress and which of my creatures are in it. Read the decision KIND
/// (globals decision one-hot) to interpret it as attacker-vs-blocker. Appended, so no index shifts.
pub const PENDING_COMBAT: usize = 1;
// (v3, OBS2_DESIGN.md §7.3) The Tier-3 relational id columns (instance/blocking/attached match
// keys, cols 45–47 of contract v2) are GONE: pairings now arrive as explicit `edges` (engine truth,
// no cross-row id-matching for the network to learn). Scalar features stay; float match-keys die.
/// Per-battlefield-row feature width (excludes `grp_id`, which rides in `bf_grpid`).
pub const F_PERM: usize =
    9 + N_CARD_TYPES + N_COLORS + N_KEYWORDS + 4 + DECISION_FLAGS + COMBAT_LINK + PENDING_COMBAT;

// Absolute bf_feat column indices of the combat/decision tail (append-stable — everything through
// BF_PENDING_COMBAT keeps its index when new columns are appended). Mirror of the push order in
// `encode_battlefield`; the Python encoder mirrors these too.
pub const BF_ATTACKING: usize = 9 + N_CARD_TYPES + N_COLORS + N_KEYWORDS + 2; // 39
pub const BF_BLOCKING: usize = BF_ATTACKING + 1; // 40
pub const BF_IS_SRC: usize = BF_ATTACKING + 2; // 41
pub const BF_IS_CAND: usize = BF_ATTACKING + 3; // 42
pub const BF_BLOCKED_BY: usize = BF_ATTACKING + 4; // 43
pub const BF_PENDING_COMBAT: usize = BF_ATTACKING + 5; // 44
/// Per-hand-row feature width.
pub const F_HAND: usize = 3 + N_CARD_TYPES + N_COLORS + DECISION_FLAGS;
/// Per-stack-row feature width.
pub const F_STACK: usize = 3 + N_CARD_TYPES + N_COLORS + DECISION_FLAGS;

/// Per-seat global scalar block: life, poison, hand, library, graveyard, exile, battlefield,
/// mana(WUBRGC).
const SEAT_BLOCK: usize = 7 + 6;
/// Global vector width. Trailing `+ 2` = decision player-candidate flags (is me / is opp a legal
/// target of the current decision).
pub const G: usize = 1 + 12 + 3 + SEAT_BLOCK + SEAT_BLOCK + 1 + NUM_REQUESTS + 3 + 2;

/// Per-choice-row feature width (v3 `choice_feat`, §7.5): col 0 `present` · 1–4 kind one-hot
/// (mode/color/number/bool) · 5 value scalar (Number rows: the number; others: the option index) ·
/// 6–10 color one-hot · 11 reserved.
pub const F_CHOICE: usize = 12;
/// Columns of the `edges` tensor: `(src_row, dst_row, type, k)`; pad rows are all −1.
pub const F_EDGE: usize = 4;

/// The structured observation. Flat `Vec`s (Python reshapes per [`spec`]); `*_grpid` are the
/// per-row card identities (`grp_id`, 0 = empty row) for the policy's embedding table — the ONLY
/// id that appears in tensors (§7.1a; entityids are resolved to row positions at encode time).
#[derive(Debug, Clone)]
pub struct Obs {
    pub globals: Vec<f32>,
    pub bf_feat: Vec<f32>,
    pub bf_grpid: Vec<i64>,
    pub hand_feat: Vec<f32>,
    pub hand_grpid: Vec<i64>,
    pub stack_feat: Vec<f32>,
    pub stack_grpid: Vec<i64>,
    /// The resolved source-card `grp_id` of the current decision (0 = none) — see module docs.
    pub decision_grpid: Vec<i64>,
    /// Relation edges `(src_row, dst_row, type, k)` in the shared row space (§7.2/§7.4), −1-padded.
    pub edges: Vec<i64>,
    /// Content tokens for the current decision's abstract options (§7.5).
    pub choice_feat: Vec<f32>,
}

/// `(name, rows, cols, is_int)` for each obs array — Python builds the `gym.spaces.Dict` from this
/// (shapes are never hard-coded on the Python side).
pub fn spec() -> Vec<(&'static str, usize, usize, bool)> {
    vec![
        ("globals", 1, G, false),
        ("bf_feat", MAX_PERM, F_PERM, false),
        ("bf_grpid", 1, MAX_PERM, true),
        ("hand_feat", MAX_HAND, F_HAND, false),
        ("hand_grpid", 1, MAX_HAND, true),
        ("stack_feat", MAX_STACK, F_STACK, false),
        ("stack_grpid", 1, MAX_STACK, true),
        // Source-card identity of the current decision (Tier 1) — one row, one grp_id.
        ("decision_grpid", 1, 1, true),
        ("edges", MAX_EDGES, F_EDGE, true),
        ("choice_feat", MAX_CHOICE, F_CHOICE, false),
    ]
}

/// In-flight sub-decision state handed over from the codec's [`Interaction`](crate::codec::Interaction)
/// — the §4a commitment prefix the frozen `view` snapshot cannot show, plus the codec's live
/// abstract-choice rows (single-sourced so obs↔codec choice alignment holds by construction).
#[derive(Default)]
pub struct PendingView {
    /// `(blocker, attacker)` pairs assigned so far in the current DeclareBlockers decision.
    pub blocks: Vec<(ObjId, ObjId)>,
    /// The blocker currently awaiting its attacker pick (lights `is_decision_source`).
    pub block_source: Option<ObjId>,
    /// My creatures already declared in the current DeclareAttackers decision.
    pub attackers: Vec<ObjId>,
    /// Targets picked so far in an in-flight multi-target decision, with pick order.
    pub target_picks: Vec<(Target, u32)>,
    /// The current decision's abstract-choice rows (from `Interaction::choice_rows`).
    pub choices: Vec<crate::codec::ChoiceRow>,
}

/// Encode `view` + the current request (and its legal-option count) into the structured [`Obs`].
///
/// `pending` carries the in-flight sub-decision state from the codec's
/// [`Interaction`](crate::codec::Interaction) — the §4a commitment prefix (pending blocks /
/// attackers / target picks) plus the codec-authored abstract-choice rows. Pass
/// `&PendingView::default()` when there is no in-flight decomposition.
pub fn encode(view: &PlayerView, req: &DecisionRequest, num_legal: usize,
              pending: &PendingView) -> Obs {
    let mut di = decision_info(view, req);
    if let Some(src) = pending.block_source {
        di.src_objs.insert(src); // (Tier 2c) light is_decision_source on the blocker being assigned
    }
    let blocked_by = blocked_by_counts(view, &pending.blocks);
    // My creatures committed in the current in-flight combat decision: declared attackers (attack
    // step) or assigned blockers incl. the one being assigned (block step) — the is_pending_combat set.
    let pending_combat: std::collections::BTreeSet<ObjId> = pending
        .attackers
        .iter()
        .copied()
        .chain(pending.blocks.iter().map(|(blk, _)| *blk))
        .chain(pending.block_source)
        .collect();
    let relations = RelationMaps::build(view, &pending.blocks);
    // The shared battlefield row ordering (nonlands-first, then lands; capped at MAX_PERM) — the SAME
    // function the action codec uses, so obs row `k` and codec `PERM[k]` name the same object even
    // when the board overflows the cap and trailing lands are dropped. See `layout::perm_order`.
    let perm_order = crate::layout::perm_order(&view.battlefield);
    let rows = RowMap::build(view, &perm_order);
    Obs {
        globals: encode_globals(view, req, num_legal, &di),
        bf_feat: encode_battlefield(view, &di, &blocked_by, &pending_combat, &perm_order),
        bf_grpid: bf_ids_ordered(&view.battlefield, &perm_order),
        hand_feat: encode_hand(view, req, &di),
        hand_grpid: ids(&view.me.hand, MAX_HAND),
        stack_feat: encode_stack(view, &di),
        stack_grpid: view
            .stack
            .iter()
            .take(MAX_STACK)
            .map(|s| s.chars.grp_id as i64)
            .chain(std::iter::repeat(0))
            .take(MAX_STACK)
            .collect(),
        decision_grpid: vec![di.src_grp],
        edges: encode_edges(view, &relations, pending, &rows),
        choice_feat: encode_choices(&pending.choices),
    }
}

/// entityid → row-space position for THIS observation (§7.2). Built once per encode; the boundary
/// where engine ids are resolved into positions and then discarded — no id crosses into a tensor.
struct RowMap {
    bf: BTreeMap<ObjId, usize>,
    hand: BTreeMap<ObjId, usize>,
    stack: BTreeMap<StackId, usize>,
    me: mtg_core::ids::PlayerId,
}

impl RowMap {
    fn build(view: &PlayerView, perm_order: &[usize]) -> RowMap {
        let mut bf = BTreeMap::new();
        for (k, &row) in perm_order.iter().enumerate() {
            bf.insert(crate::layout::objview_id(&view.battlefield[row]), k);
        }
        let mut hand = BTreeMap::new();
        for (i, o) in view.me.hand.iter().take(MAX_HAND).enumerate() {
            hand.insert(crate::layout::objview_id(o), ROW_HAND + i);
        }
        let mut stack = BTreeMap::new();
        for (i, s) in view.stack.iter().take(MAX_STACK).enumerate() {
            stack.insert(s.id, ROW_STACK + i);
        }
        RowMap { bf, hand, stack, me: view.seat }
    }
    fn obj(&self, id: ObjId) -> Option<usize> {
        self.bf.get(&id).copied().or_else(|| self.hand.get(&id).copied())
    }
    fn target(&self, t: &Target) -> Option<usize> {
        match t {
            Target::Object(id) => self.obj(*id),
            Target::Stack(sid) => self.stack.get(sid).copied(),
            Target::Player(p) => Some(if *p == self.me { ROW_ME } else { ROW_OPP }),
        }
    }
}

/// Relation edges `(src_row, dst_row, type, k)` (§7.4), emitted in truncation-priority order
/// (TARGETS > PENDING_PICK > BLOCKS > ATTACHED_TO > ATTACKS > STACK_SOURCE) and −1-padded to
/// [`MAX_EDGES`]. Pending relations appear in their final type immediately (a mid-decision block
/// already has its BLOCKS edge); pendingness is marked by the accompanying PENDING_PICK edge.
fn encode_edges(view: &PlayerView, relations: &RelationMaps, pending: &PendingView,
                rows: &RowMap) -> Vec<i64> {
    let mut e: Vec<[i64; 4]> = Vec::new();
    // TARGETS: what each stack object targets (closes gap G1). k = target slot order.
    for (si, s) in view.stack.iter().take(MAX_STACK).enumerate() {
        for (k, t) in s.targets.iter().enumerate() {
            if let Some(dst) = rows.target(t) {
                e.push([(ROW_STACK + si) as i64, dst as i64, EDGE_TARGETS, k as i64]);
            }
        }
    }
    // PENDING_PICK: decision → every pick already made in the in-flight decision (§4a). k = order.
    let mut k: i64 = 0;
    let pend = |e: &mut Vec<[i64; 4]>, row: Option<usize>, k: &mut i64| {
        if let Some(r) = row {
            e.push([ROW_DECISION as i64, r as i64, EDGE_PENDING_PICK, *k]);
            *k += 1;
        }
    };
    for id in &pending.attackers {
        pend(&mut e, rows.bf.get(id).copied(), &mut k);
    }
    for (blk, _) in &pending.blocks {
        pend(&mut e, rows.bf.get(blk).copied(), &mut k);
    }
    if let Some(src) = pending.block_source {
        pend(&mut e, rows.bf.get(&src).copied(), &mut k);
    }
    for (t, _) in &pending.target_picks {
        pend(&mut e, rows.target(t), &mut k);
    }
    // BLOCKS: blocker → the attacker it blocks (committed + pending, from RelationMaps).
    for (blk, atk) in &relations.blocking_of {
        if let (Some(b), Some(a)) = (rows.bf.get(blk), rows.bf.get(atk)) {
            e.push([*b as i64, *a as i64, EDGE_BLOCKS, 0]);
        }
    }
    // ATTACHED_TO: aura/equipment → host.
    for (att, host) in &relations.attached_of {
        if let (Some(a), Some(h)) = (rows.bf.get(att), rows.bf.get(host)) {
            e.push([*a as i64, *h as i64, EDGE_ATTACHED_TO, 0]);
        }
    }
    // ATTACKS: committed attacker → whom it attacks (a player today; planeswalkers later).
    if let Some(c) = &view.combat {
        for (a, t) in &c.attackers {
            if let (Some(ar), Some(tr)) = (rows.bf.get(a), rows.target(t)) {
                e.push([*ar as i64, tr as i64, EDGE_ATTACKS, 0]);
            }
        }
    }
    // STACK_SOURCE: an ability on the stack → the permanent it came from.
    for (si, s) in view.stack.iter().take(MAX_STACK).enumerate() {
        if let Some(src) = s.source {
            if let Some(r) = rows.obj(src) {
                e.push([(ROW_STACK + si) as i64, r as i64, EDGE_STACK_SOURCE, 0]);
            }
        }
    }
    e.truncate(MAX_EDGES);
    let n = e.len();
    let mut out = Vec::with_capacity(MAX_EDGES * F_EDGE);
    for row in &e {
        out.extend_from_slice(row);
    }
    out.extend(std::iter::repeat(-1).take((MAX_EDGES - n) * F_EDGE));
    out
}

/// The current decision's abstract options as content tokens (§7.5). Rows come verbatim from the
/// codec ([`crate::codec::Interaction::choice_rows`]) so `MODE[j]`/`COLOR[j]`/`NUMBER[j]`/YES/NO
/// slot `j` and choice row `j` can never disagree.
fn encode_choices(choices: &[crate::codec::ChoiceRow]) -> Vec<f32> {
    use crate::codec::ChoiceKind as K;
    let mut out = vec![0.0; MAX_CHOICE * F_CHOICE];
    for c in choices {
        if c.row >= MAX_CHOICE {
            continue;
        }
        let b = c.row * F_CHOICE;
        out[b] = 1.0; // present
        let kind = match c.kind {
            K::Mode => 0,
            K::Color => 1,
            K::Number => 2,
            K::Bool => 3,
        };
        out[b + 1 + kind] = 1.0;
        out[b + 5] = c.value;
        if let Some(ci) = c.color {
            if ci < N_COLORS {
                out[b + 6 + ci] = 1.0;
            }
        }
        // col 11 reserved
    }
    out
}

/// Which objects/players the *current* decision is **for** (its source) and **over** (its legal
/// candidates) — the Tier-1 signal that ties a decision to the spell/ability that raised it. All of
/// this is already present in the `DecisionRequest` + `PlayerView`; the encoder just surfaces it so
/// the policy needn't infer "the source is whatever's on top of the stack".
#[derive(Default)]
struct DecisionInfo {
    /// Object ids (battlefield/hand rows) that are the SOURCE of the current decision.
    src_objs: BTreeSet<ObjId>,
    /// The stack object that is the source (a spell/ability mid-resolution, by `StackId`).
    src_stack: Option<mtg_core::ids::StackId>,
    /// Resolved source CARD identity (`grp_id`): a spell's own id, else an ability followed back to
    /// its source permanent's id; 0 when the decision has no identifiable source.
    src_grp: i64,
    /// Object ids that are legal CANDIDATES of the current decision (targets / selectable cards).
    cand_objs: BTreeSet<ObjId>,
    /// Stack ids that are legal candidates (targeting a spell/ability on the stack — #47).
    cand_stack: BTreeSet<mtg_core::ids::StackId>,
    /// Whether each player is a legal candidate/target right now.
    cand_me: bool,
    cand_opp: bool,
}

fn add_target(di: &mut DecisionInfo, me: mtg_core::ids::PlayerId, t: &mtg_core::basics::Target) {
    use mtg_core::basics::Target;
    match t {
        Target::Object(id) => {
            di.cand_objs.insert(*id);
        }
        Target::Stack(sid) => {
            di.cand_stack.insert(*sid);
        }
        Target::Player(p) => {
            if *p == me {
                di.cand_me = true;
            } else {
                di.cand_opp = true;
            }
        }
    }
}

/// The `grp_id` of a visible object found on the battlefield or in our hand (for resolving an
/// ability's source permanent to a card identity).
fn grp_of_obj(view: &PlayerView, id: ObjId) -> Option<i64> {
    view.battlefield
        .iter()
        .chain(view.me.hand.iter())
        .find_map(|o| match o {
            ObjView::Visible { id: oid, chars, .. } if *oid == id => Some(chars.grp_id as i64),
            _ => None,
        })
}

fn decision_info(view: &PlayerView, req: &DecisionRequest) -> DecisionInfo {
    use DecisionRequest as Q;
    let me = view.seat;
    let mut di = DecisionInfo::default();

    // ── source: the in-progress cast/activation a sub-decision belongs to ──────────────────────
    // Two complementary signals: (1) match `for_action`'s StackId against the stack — resolves a
    // SPELL mid-cast (its stack row carries the card's grp_id) and an ACTIVATED ability (pushed
    // before its targets); (2) the request's explicit `source` object (CR 603.3d: a TRIGGERED/
    // reflexive ability chooses targets BEFORE it's pushed onto the stack, so it isn't in the view
    // yet — the engine hands us the source object directly so we can still recover its identity).
    let (for_action, req_source) = match req {
        Q::ChooseTargets { for_action, source, .. } => (Some(for_action.0), *source),
        Q::ChooseModes { for_action, .. } | Q::CastingTimeOptions { for_action, .. } => {
            (Some(for_action.0), None)
        }
        _ => (None, None),
    };
    if let Some(sid) = for_action {
        di.src_stack = Some(sid);
        if let Some(s) = view.stack.iter().find(|s| s.id == sid) {
            if s.chars.grp_id != 0 {
                di.src_grp = s.chars.grp_id as i64; // a spell carries its own card identity
            }
            if let Some(src) = s.source {
                di.src_objs.insert(src); // an ability's source permanent (its stack row is blank)
                if di.src_grp == 0 {
                    if let Some(g) = grp_of_obj(view, src) {
                        di.src_grp = g;
                    }
                }
            }
        }
    }
    // The request's explicit source — works for a trigger whose object isn't on the stack yet.
    if let Some(src) = req_source {
        di.src_objs.insert(src); // flag its battlefield/hand row as the decision's source
        if di.src_grp == 0 {
            if let Some(g) = grp_of_obj(view, src) {
                di.src_grp = g;
            }
        }
    }
    if let Q::AssignCombatDamage { source, .. } = req {
        di.src_objs.insert(*source);
        if let Some(g) = grp_of_obj(view, *source) {
            di.src_grp = g;
        }
    }

    // ── candidates: the objects/players this decision chooses among ────────────────────────────
    match req {
        Q::ChooseTargets { slots, .. } => {
            for slot in slots {
                for t in &slot.legal {
                    add_target(&mut di, me, t);
                }
            }
        }
        Q::Distribute { among, .. } => {
            for t in among {
                add_target(&mut di, me, t);
            }
        }
        Q::AssignCombatDamage { recipients, .. } => {
            for r in recipients {
                add_target(&mut di, me, &r.recipient);
            }
        }
        Q::SelectCards { from, .. } => {
            di.cand_objs.extend(from.iter().copied());
        }
        Q::SelectFromGroups { groups, .. } => {
            for g in groups {
                di.cand_objs.extend(g.options.iter().copied());
            }
        }
        Q::DeclareAttackers { eligible } => {
            di.cand_objs.extend(eligible.iter().map(|a| a.creature));
        }
        Q::DeclareBlockers { eligible, attackers } => {
            di.cand_objs.extend(eligible.iter().map(|b| b.creature));
            di.cand_objs.extend(attackers.iter().copied());
        }
        Q::OrderObjects { items, .. } => {
            di.cand_objs.extend(items.iter().copied());
        }
        _ => {}
    }
    di
}

fn ids(objs: &[ObjView], max: usize) -> Vec<i64> {
    objs.iter()
        .take(max)
        .map(grp_of)
        .chain(std::iter::repeat(0))
        .take(max)
        .collect()
}

fn grp_of(o: &ObjView) -> i64 {
    match o {
        ObjView::Visible { chars, .. } => chars.grp_id as i64,
        ObjView::Hidden { .. } => 0,
    }
}

fn encode_globals(
    view: &PlayerView,
    req: &DecisionRequest,
    num_legal: usize,
    di: &DecisionInfo,
) -> Vec<f32> {
    let mut o = Vec::with_capacity(G);
    o.push(view.turn as f32);
    for ph in PHASES {
        o.push((view.phase == ph) as u8 as f32);
    }
    let me = view.seat;
    o.push((view.active_player == me) as u8 as f32);
    o.push((view.priority_player == Some(me)) as u8 as f32);
    o.push(view.priority_player.is_some() as u8 as f32);

    let my = view.players.iter().find(|p| p.player == me);
    let opp = view.players.iter().find(|p| p.player != me);
    push_seat(&mut o, my, view.me.hand.len() as u32, view);
    push_seat(&mut o, opp, opp.map(|p| p.hand_count).unwrap_or(0), view);

    o.push(view.stack.len() as f32);

    let ridx = request_index(req);
    for i in 0..NUM_REQUESTS {
        o.push((i == ridx) as u8 as f32);
    }
    let (lo, hi) = request_bounds(req);
    o.push(num_legal as f32);
    o.push(lo);
    o.push(hi);

    // Decision player-candidate flags: is each player a legal target of the current decision.
    o.push(di.cand_me as u8 as f32);
    o.push(di.cand_opp as u8 as f32);

    debug_assert_eq!(o.len(), G);
    o
}

fn push_seat(o: &mut Vec<f32>, seat: Option<&PlayerPublicView>, hand: u32, view: &PlayerView) {
    match seat {
        Some(p) => {
            o.push(p.life as f32);
            o.push(p.poison as f32);
            o.push(hand as f32);
            o.push(p.library_count as f32);
            o.push(p.graveyard.len() as f32);
            o.push(p.exile_public.len() as f32);
            o.push(
                view.battlefield
                    .iter()
                    .filter(|ov| controller_of(ov) == Some(p.player))
                    .count() as f32,
            );
            for c in [
                Color::White,
                Color::Blue,
                Color::Black,
                Color::Red,
                Color::Green,
                Color::Colorless,
            ] {
                o.push(p.mana_pool.amounts.get(&c).copied().unwrap_or(0) as f32);
            }
        }
        None => o.extend(std::iter::repeat(0.0).take(SEAT_BLOCK)),
    }
}

fn controller_of(o: &ObjView) -> Option<mtg_core::ids::PlayerId> {
    match o {
        ObjView::Visible { controller, .. } => Some(*controller),
        ObjView::Hidden { controller, .. } => Some(*controller),
    }
}

/// Per-attacker "blocked-by count": how many blockers gang each attacker, from BOTH the engine's
/// committed blocks (`view.combat.blockers`) AND the blocks assigned so far in the current
/// DeclareBlockers decision (`pending_blocks`, from the in-flight Interaction). During the decision the
/// committed set is empty and pending fills up; after it commits, pending is empty and the committed
/// set carries them — so the sum is right in both phases without double-counting.
fn blocked_by_counts(view: &PlayerView, pending_blocks: &[(ObjId, ObjId)])
    -> std::collections::BTreeMap<ObjId, u32> {
    let mut m = std::collections::BTreeMap::new();
    if let Some(c) = &view.combat {
        for (_blk, atk) in &c.blockers {
            *m.entry(*atk).or_insert(0) += 1;
        }
    }
    for (_blk, atk) in pending_blocks {
        *m.entry(*atk).or_insert(0) += 1;
    }
    m
}

/// Relation maps feeding the `edges` tensor: for each blocker, the attacker it blocks (committed +
/// pending); for each attachment, its host permanent. Object ids only — data-only, no characteristics.
struct RelationMaps {
    blocking_of: std::collections::BTreeMap<ObjId, ObjId>,   // blocker  -> attacker it blocks
    attached_of: std::collections::BTreeMap<ObjId, ObjId>,   // attachment -> host permanent
}

impl RelationMaps {
    fn build(view: &PlayerView, pending_blocks: &[(ObjId, ObjId)]) -> RelationMaps {
        let mut blocking_of = std::collections::BTreeMap::new();
        if let Some(c) = &view.combat {
            for (blk, atk) in &c.blockers {
                blocking_of.insert(*blk, *atk);
            }
        }
        for (blk, atk) in pending_blocks {
            blocking_of.insert(*blk, *atk); // pending assignment overrides/adds the same as committed
        }
        let mut attached_of = std::collections::BTreeMap::new();
        for o in &view.battlefield {
            if let ObjView::Visible { id, attachments, .. } = o {
                for att in attachments {
                    attached_of.insert(*att, *id);
                }
            }
        }
        RelationMaps { blocking_of, attached_of }
    }
}

fn encode_battlefield(view: &PlayerView, di: &DecisionInfo,
                      blocked_by: &std::collections::BTreeMap<ObjId, u32>,
                      pending_combat: &std::collections::BTreeSet<ObjId>,
                      perm_order: &[usize]) -> Vec<f32> {
    let me = view.seat;
    let (attacking, blocking) = combat_sets(view);
    let mut out = Vec::with_capacity(MAX_PERM * F_PERM);
    for &row in perm_order {
        let o = &view.battlefield[row];
        match o {
            ObjView::Visible {
                id,
                chars,
                controller,
                status,
                counters,
                damage_marked,
                attachments,
                summoning_sick,
                ..
            } => {
                out.push(1.0); // present
                out.push((*controller == me) as u8 as f32);
                out.push(chars.power.unwrap_or(0) as f32);
                out.push(chars.toughness.unwrap_or(0) as f32);
                out.push(chars.mana_value as f32);
                out.push(*damage_marked as f32);
                out.push(status.tapped as u8 as f32);
                out.push(*summoning_sick as u8 as f32);
                out.push(status.face_down as u8 as f32);
                push_types(&mut out, chars);
                push_colors(&mut out, chars);
                push_keywords(&mut out, chars);
                out.push(counters.counts.values().sum::<u32>() as f32);
                out.push(attachments.len() as f32);
                out.push(attacking.contains(id) as u8 as f32);
                out.push(blocking.contains(id) as u8 as f32);
                out.push(di.src_objs.contains(id) as u8 as f32); // source of current decision
                out.push(di.cand_objs.contains(id) as u8 as f32); // legal candidate of it
                // (Tier 2) blocked-by count: blockers ganging this attacker (committed + pending).
                out.push(*blocked_by.get(id).unwrap_or(&0) as f32);
                // (Tier 2b) is_pending_combat: this creature is mine and already committed in the
                // current in-flight combat decision (declared attacker / assigned blocker).
                // Pairings (who blocks whom, aura hosts) are NOT columns — they ride `edges` (v3).
                out.push(pending_combat.contains(id) as u8 as f32);
            }
            ObjView::Hidden { .. } => {
                // Hidden permanent (e.g. a face-down): present but featureless.
                out.push(1.0);
                out.extend(std::iter::repeat(0.0).take(F_PERM - 1));
            }
        }
    }
    out.extend(std::iter::repeat(0.0).take((MAX_PERM - perm_order.len()) * F_PERM));
    out
}

/// Battlefield `grp_id`s in the shared `perm_order` (so `bf_grpid[k]` matches obs row `k` / codec
/// `PERM[k]`), padded to MAX_PERM with 0. Distinct from the generic `ids` (used for the natural-order
/// hand table), which does NOT reorder.
fn bf_ids_ordered(battlefield: &[ObjView], perm_order: &[usize]) -> Vec<i64> {
    perm_order
        .iter()
        .map(|&row| grp_of(&battlefield[row]))
        .chain(std::iter::repeat(0))
        .take(MAX_PERM)
        .collect()
}

fn encode_hand(view: &PlayerView, req: &DecisionRequest, di: &DecisionInfo) -> Vec<f32> {
    let castable = castable_set(req);
    let mut out = Vec::with_capacity(MAX_HAND * F_HAND);
    for o in view.me.hand.iter().take(MAX_HAND) {
        if let ObjView::Visible { id, chars, .. } = o {
            out.push(1.0);
            out.push(chars.mana_value as f32);
            out.push(castable.contains(id) as u8 as f32);
            push_types(&mut out, chars);
            push_colors(&mut out, chars);
            out.push(di.src_objs.contains(id) as u8 as f32); // source of current decision
            out.push(di.cand_objs.contains(id) as u8 as f32); // legal candidate (e.g. discard/reveal)
        } else {
            out.push(1.0);
            out.extend(std::iter::repeat(0.0).take(F_HAND - 1));
        }
    }
    out.extend(std::iter::repeat(0.0).take((MAX_HAND - view.me.hand.len().min(MAX_HAND)) * F_HAND));
    out
}

fn encode_stack(view: &PlayerView, di: &DecisionInfo) -> Vec<f32> {
    let me = view.seat;
    let mut out = Vec::with_capacity(MAX_STACK * F_STACK);
    for s in view.stack.iter().take(MAX_STACK) {
        out.push(1.0);
        out.push((s.controller == me) as u8 as f32);
        out.push(s.chars.mana_value as f32);
        push_types(&mut out, &s.chars);
        push_colors(&mut out, &s.chars);
        out.push((di.src_stack == Some(s.id)) as u8 as f32); // the spell/ability being decided
        out.push(di.cand_stack.contains(&s.id) as u8 as f32); // a stack object being targeted (#47)
    }
    out.extend(std::iter::repeat(0.0).take((MAX_STACK - view.stack.len().min(MAX_STACK)) * F_STACK));
    out
}

fn push_types(out: &mut Vec<f32>, chars: &CharacteristicsView) {
    for t in CARD_TYPES {
        out.push(chars.card_types.iter().any(|x| x == t) as u8 as f32);
    }
}
fn push_colors(out: &mut Vec<f32>, chars: &CharacteristicsView) {
    for c in COLORS {
        out.push(chars.colors.contains(&c) as u8 as f32);
    }
}
fn push_keywords(out: &mut Vec<f32>, chars: &CharacteristicsView) {
    for k in KEYWORDS {
        out.push(chars.keywords.iter().any(|x| x == k) as u8 as f32);
    }
}

/// Attacking / blocking object id sets from the combat view (for the per-permanent combat role).
fn combat_sets(view: &PlayerView) -> (BTreeSet<ObjId>, BTreeSet<ObjId>) {
    let mut atk = BTreeSet::new();
    let mut blk = BTreeSet::new();
    if let Some(c) = &view.combat {
        for (a, _) in &c.attackers {
            atk.insert(*a);
        }
        for (b, _) in &c.blockers {
            blk.insert(*b);
        }
    }
    (atk, blk)
}

/// The hand cards the engine currently lists as playable (castable flag), from a `Priority` req.
fn castable_set(req: &DecisionRequest) -> BTreeSet<ObjId> {
    use mtg_core::agent::PlayableAction as A;
    let mut s = BTreeSet::new();
    if let DecisionRequest::Priority { actions, .. } = req {
        for a in actions {
            match a {
                A::Cast { spell, .. } => {
                    s.insert(*spell);
                }
                A::PlayLand { card } => {
                    s.insert(*card);
                }
                _ => {}
            }
        }
    }
    s
}

/// Two request scalars (a generic min/max) so the policy can respect bounds without re-deriving.
fn request_bounds(req: &DecisionRequest) -> (f32, f32) {
    use DecisionRequest as Q;
    let (lo, hi) = match req {
        Q::ChooseNumber { min, max, .. } => (*min as f32, *max as f32),
        Q::SelectCards { min, max, .. } => (*min as f32, *max as f32),
        Q::ChooseModes { min, max, .. } => (*min as f32, *max as f32),
        Q::ChooseOption { min, max, .. } => (*min as f32, *max as f32),
        Q::ChooseColor { min, max, .. } => (*min as f32, *max as f32),
        Q::Distribute { total, .. } => (0.0, *total as f32),
        _ => (0.0, 0.0),
    };
    (lo, hi)
}

/// Stable index of each request variant (matches `crate::request_name` ordering).
pub fn request_index(req: &DecisionRequest) -> usize {
    use DecisionRequest as Q;
    match req {
        Q::ChooseStartingPlayer { .. } => 0,
        Q::Mulligan { .. } => 1,
        Q::Priority { .. } => 2,
        Q::ChooseModes { .. } => 3,
        Q::ChooseNumber { .. } => 4,
        Q::CastingTimeOptions { .. } => 5,
        Q::ChooseTargets { .. } => 6,
        Q::Distribute { .. } => 7,
        Q::PayCost { .. } => 8,
        Q::DeclareAttackers { .. } => 9,
        Q::DeclareBlockers { .. } => 10,
        Q::AssignCombatDamage { .. } => 11,
        Q::OrderObjects { .. } => 12,
        Q::SelectCards { .. } => 13,
        Q::SelectFromGroups { .. } => 14,
        Q::ArrangeCards { .. } => 15,
        Q::ChooseReplacement { .. } => 16,
        Q::ChooseCounterType { .. } => 17,
        Q::ChooseOption { .. } => 18,
        Q::ChooseColor { .. } => 19,
        Q::Confirm { .. } => 20,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::agent::{PlayerPrivateView, PlayerPublicView};
    use mtg_core::basics::{CounterBag, ManaPool};
    use mtg_core::ids::PlayerId;

    fn pub_view(p: u32, life: i32) -> PlayerPublicView {
        PlayerPublicView {
            player: PlayerId(p),
            life,
            poison: 0,
            hand_count: 2,
            library_count: 30,
            graveyard: vec![],
            exile_public: vec![],
            mana_pool: ManaPool::default(),
            counters: CounterBag::default(),
        }
    }

    fn view() -> PlayerView {
        PlayerView {
            seat: PlayerId(0),
            turn: 3,
            active_player: PlayerId(0),
            phase: Phase::PrecombatMain,
            priority_player: Some(PlayerId(0)),
            players: vec![pub_view(0, 20), pub_view(1, 18)],
            me: PlayerPrivateView { hand: vec![], known_library: vec![], revealed_to_me: vec![] },
            battlefield: vec![],
            stack: vec![],
            combat: None,
            stops: None,
        }
    }

    #[test]
    fn shapes_match_spec_and_are_finite() {
        let req = DecisionRequest::Priority { actions: vec![], can_pass: true };
        let o = encode(&view(), &req, 1, &PendingView::default());
        assert_eq!(o.globals.len(), G);
        assert_eq!(o.bf_feat.len(), MAX_PERM * F_PERM);
        assert_eq!(o.bf_grpid.len(), MAX_PERM);
        assert_eq!(o.hand_feat.len(), MAX_HAND * F_HAND);
        assert_eq!(o.hand_grpid.len(), MAX_HAND);
        assert_eq!(o.stack_feat.len(), MAX_STACK * F_STACK);
        assert_eq!(o.stack_grpid.len(), MAX_STACK);
        assert_eq!(o.edges.len(), MAX_EDGES * F_EDGE);
        assert_eq!(o.choice_feat.len(), MAX_CHOICE * F_CHOICE);
        assert!(o.globals.iter().all(|x| x.is_finite()));
        assert!(o.bf_feat.iter().all(|x| x.is_finite()));
        // No relations, nothing pending → every edge row is padding (−1).
        assert!(o.edges.iter().all(|&x| x == -1), "empty board → all edge rows padded");
        // turn first; PrecombatMain one-hot set; self life (20) leads the me-block.
        assert_eq!(o.globals[0], 3.0);
        assert_eq!(o.globals[1 + 3], 1.0);
    }

    #[test]
    fn spec_total_matches_encoded_lengths() {
        for (_name, rows, cols, _is_int) in spec() {
            assert!(rows * cols > 0);
        }
        // F_PERM / F_HAND / F_STACK pinned so a vocab edit that desyncs obs↔codec is caught.
        // (DECISION_FLAGS=2 Tier-1 flags; COMBAT_LINK=1 blocked-by count; PENDING_COMBAT=1 self-flag.
        // Contract v3: the Tier-3 relation-id columns are gone — pairings ride `edges` now.)
        assert_eq!(F_PERM, 45);
        assert_eq!(BF_PENDING_COMBAT, F_PERM - 1); // the combat/decision tail closes the row
        assert_eq!(F_HAND, 18);
        assert_eq!(F_STACK, 18);
        assert_eq!(G, 69);
        assert_eq!(F_CHOICE, 12);
        assert_eq!(F_EDGE, 4);
    }

    /// Tier 1: a `ChooseTargets` raised by a spell mid-cast surfaces (a) the spell's card identity
    /// in `decision_ids`, (b) the source stack row flagged, and (c) the legal candidates flagged on
    /// the rows they live on (a battlefield creature + the opponent as a player-candidate).
    #[test]
    fn decision_surfaces_source_and_candidates() {
        use mtg_core::agent::{
            ActionRef, CharacteristicsView, StackObjView, TargetSlot,
        };
        use mtg_core::basics::{Status, Target};
        use mtg_core::ids::{ObjId, StackId};

        let creature = ObjId(10); // a battlefield creature that is a legal target
        let spell_card = ObjId(20); // the Erode-like spell, now on the stack
        let mut v = view();
        v.battlefield = vec![ObjView::Visible {
            id: creature,
            chars: CharacteristicsView { grp_id: 700, power: Some(2), toughness: Some(2), ..Default::default() },
            controller: PlayerId(1), // an opponent's creature
            owner: PlayerId(1),
            zone: mtg_core::basics::Zone::Battlefield,
            status: Status::default(),
            counters: CounterBag::default(),
            damage_marked: 0,
            attachments: vec![],
            summoning_sick: false,
        }];
        v.stack = vec![StackObjView {
            id: StackId(1),
            controller: PlayerId(0),
            source: Some(spell_card),
            chars: CharacteristicsView { grp_id: 555, ..Default::default() }, // a spell → carries its id
            targets: vec![],
        }];
        let req = DecisionRequest::ChooseTargets {
            for_action: ActionRef(StackId(1)),
            source: Some(spell_card),
            slots: vec![TargetSlot {
                description: String::new(),
                legal: vec![Target::Object(creature), Target::Player(PlayerId(1))],
                min: 1,
                max: 1,
            }],
        };

        let o = encode(&v, &req, 2, &PendingView::default());
        // (a) source card identity = the spell's grp_id.
        assert_eq!(o.decision_grpid, vec![555], "decision_grpid carries the source spell's grp_id");
        // (b) the stack row (row 0) is flagged is_src (the last-but-one feature of F_STACK).
        assert_eq!(o.stack_feat[F_STACK - 2], 1.0, "source spell's stack row flagged is_src");
        // (c) the creature row (row 0) is flagged is_cand (absolute column, append-stable).
        assert_eq!(o.bf_feat[BF_IS_CAND], 1.0, "legal-target creature flagged is_cand");
        assert_eq!(o.bf_feat[BF_IS_SRC], 0.0, "the opponent's creature is not the source");
        // and the opponent is a player-candidate (last global), self is not (second-last).
        assert_eq!(o.globals[G - 1], 1.0, "opponent is a legal player target");
        assert_eq!(o.globals[G - 2], 0.0, "self is not a legal target here");
    }

    /// Tier-1 trigger case (#66 follow-up, engine 6fe1580): a TRIGGERED ability chooses its target
    /// *before* being pushed to the stack, so the source object isn't in `view.stack`. The request's
    /// explicit `source` lets the encoder still recover the source permanent's card identity and flag
    /// its battlefield row — this is the Earthbender reflexive-reward case the user asked about.
    #[test]
    fn decision_resolves_trigger_source_off_stack() {
        use mtg_core::agent::{ActionRef, CharacteristicsView, TargetSlot};
        use mtg_core::basics::{Status, Target};
        use mtg_core::ids::{ObjId, StackId};

        let enchantment = ObjId(30); // the triggering permanent (e.g. Earthbender), grp 114
        let my_creature = ObjId(31); // the legal target (a creature you control)
        let mut v = view();
        v.battlefield = vec![
            ObjView::Visible {
                id: enchantment,
                chars: CharacteristicsView { grp_id: 114, ..Default::default() },
                controller: PlayerId(0),
                owner: PlayerId(0),
                zone: mtg_core::basics::Zone::Battlefield,
                status: Status::default(),
                counters: CounterBag::default(),
                damage_marked: 0,
                attachments: vec![],
                summoning_sick: false,
            },
            ObjView::Visible {
                id: my_creature,
                chars: CharacteristicsView { grp_id: 200, power: Some(2), toughness: Some(2), ..Default::default() },
                controller: PlayerId(0),
                owner: PlayerId(0),
                zone: mtg_core::basics::Zone::Battlefield,
                status: Status::default(),
                counters: CounterBag::default(),
                damage_marked: 0,
                attachments: vec![],
                summoning_sick: false,
            },
        ];
        v.stack = vec![]; // the trigger is NOT on the stack yet (CR 603.3d)
        let req = DecisionRequest::ChooseTargets {
            for_action: ActionRef(StackId(9)), // a StackId not present in the (empty) stack
            source: Some(enchantment),
            slots: vec![TargetSlot {
                description: String::new(),
                legal: vec![Target::Object(my_creature)],
                min: 1,
                max: 1,
            }],
        };

        let o = encode(&v, &req, 1, &PendingView::default());
        assert_eq!(o.decision_grpid, vec![114], "source grp recovered from the off-stack trigger source");
        // row 0 = the enchantment, flagged is_src; row 1 = the creature, flagged is_cand (absolute).
        assert_eq!(o.bf_feat[BF_IS_SRC], 1.0, "triggering permanent flagged is_src");
        assert_eq!(o.bf_feat[F_PERM + BF_IS_CAND], 1.0, "target creature flagged is_cand");
    }

    /// Tier 2 — the learnability proof: the per-attacker blocked-by count on an attacker's row
    /// distinguishes 0 (unblocked) / 1 (single-block) / 2 (GANGED). Without this the obs can't tell a
    /// deliberate double-block from a chump single-block — the frozen view snapshot never shows the
    /// pending assignments. `block_source` also lights is_decision_source on the blocker being assigned.
    #[test]
    fn blocked_by_count_makes_ganging_observable() {
        use mtg_core::agent::{BlockerOption, CharacteristicsView};
        use mtg_core::basics::Status;
        use mtg_core::ids::ObjId;

        let attacker = ObjId(30); // an opponent 3/3 trampler on the battlefield (row 0)
        let blk_a = ObjId(10);
        let blk_b = ObjId(11);
        let mut v = view();
        v.battlefield = vec![ObjView::Visible {
            id: attacker,
            chars: CharacteristicsView { grp_id: 300, power: Some(3), toughness: Some(3), ..Default::default() },
            controller: PlayerId(1),
            owner: PlayerId(1),
            zone: mtg_core::basics::Zone::Battlefield,
            status: Status::default(),
            counters: CounterBag::default(),
            damage_marked: 0,
            attachments: vec![],
            summoning_sick: false,
        }];
        let req = DecisionRequest::DeclareBlockers {
            eligible: vec![
                BlockerOption { creature: blk_a, may_block: vec![attacker], required: false, block_cost: None },
                BlockerOption { creature: blk_b, may_block: vec![attacker], required: false, block_cost: None },
            ],
            attackers: vec![attacker],
        };
        let pend = |blocks: Vec<(ObjId, ObjId)>, src: Option<ObjId>| PendingView {
            blocks, block_source: src, ..Default::default()
        };
        // No blocks assigned yet → 0 on the attacker's row.
        assert_eq!(encode(&v, &req, 3, &PendingView::default()).bf_feat[BF_BLOCKED_BY], 0.0, "unblocked → 0");
        // One blocker → single-block → 1.
        assert_eq!(encode(&v, &req, 3, &pend(vec![(blk_a, attacker)], None)).bf_feat[BF_BLOCKED_BY], 1.0, "single → 1");
        // TWO blockers ganging the SAME attacker (pending) → 2: double-blocking is now observable.
        let o2 = encode(&v, &req, 3, &pend(vec![(blk_a, attacker), (blk_b, attacker)], Some(blk_b)));
        assert_eq!(o2.bf_feat[BF_BLOCKED_BY], 2.0, "pending gang → 2 (the double-block signal)");
    }

    /// Tier 2b — the is_pending_combat self-flag: at DeclareBlockers my creatures already assigned as
    /// blockers (pending) light the flag on their own rows; at DeclareAttackers my declared attackers
    /// do. This is the mid-declaration self-view the action mask can't give the value/feature net.
    #[test]
    fn is_pending_combat_flags_my_committed_combatants() {
        use mtg_core::agent::{AttackerOption, BlockerOption, CharacteristicsView};
        use mtg_core::basics::{Status, Target};
        use mtg_core::ids::ObjId;

        let my_creature = ObjId(10); // mine — the blocker/attacker (row 0)
        let enemy = ObjId(30); // opponent's attacker (row 1)
        let mut v = view();
        let vis = |id, ctrl: u32| ObjView::Visible {
            id,
            chars: CharacteristicsView { grp_id: 1, power: Some(2), toughness: Some(2), ..Default::default() },
            controller: PlayerId(ctrl),
            owner: PlayerId(ctrl),
            zone: mtg_core::basics::Zone::Battlefield,
            status: Status::default(),
            counters: CounterBag::default(),
            damage_marked: 0,
            attachments: vec![],
            summoning_sick: false,
        };
        v.battlefield = vec![vis(my_creature, 0), vis(enemy, 1)];
        let pc = BF_PENDING_COMBAT;

        // DeclareBlockers: my_creature pending-assigned to block `enemy` → flagged; enemy is not mine.
        let blk_req = DecisionRequest::DeclareBlockers {
            eligible: vec![BlockerOption { creature: my_creature, may_block: vec![enemy], required: false, block_cost: None }],
            attackers: vec![enemy],
        };
        let ob = encode(&v, &blk_req, 3,
                        &PendingView { blocks: vec![(my_creature, enemy)], ..Default::default() });
        assert_eq!(ob.bf_feat[pc], 1.0, "my pending blocker flagged is_pending_combat");
        assert_eq!(ob.bf_feat[F_PERM + pc], 0.0, "the enemy attacker is not my pending combatant");

        // DeclareAttackers: my_creature declared as an attacker → flagged via pending_attackers.
        let atk_req = DecisionRequest::DeclareAttackers {
            eligible: vec![AttackerOption { creature: my_creature, may_attack: vec![Target::Player(PlayerId(1))], required: false, attack_cost: None, may_exert: false, may_enlist: false }],
        };
        let oa = encode(&v, &atk_req, 2,
                        &PendingView { attackers: vec![my_creature], ..Default::default() });
        assert_eq!(oa.bf_feat[pc], 1.0, "my declared attacker flagged is_pending_combat");
    }

    /// Decode the −1-padded flat edges vec into (src, dst, type, k) tuples for assertions.
    fn edge_tuples(o: &Obs) -> Vec<(i64, i64, i64, i64)> {
        o.edges
            .chunks(F_EDGE)
            .take_while(|c| c[0] >= 0)
            .map(|c| (c[0], c[1], c[2], c[3]))
            .collect()
    }

    /// v3 §7.4 — relations arrive as explicit edges in row space: a mid-decision block emits BOTH
    /// its BLOCKS edge (final type, immediately) AND a PENDING_PICK edge from the decision token
    /// (the §4a commitment prefix); an aura emits ATTACHED_TO → its host. Raw entityids appear in
    /// NO tensor — the RowMap resolves them to row positions at encode time.
    #[test]
    fn edges_link_blocker_attacker_aura_host_and_pending() {
        use mtg_core::agent::{BlockerOption, CharacteristicsView};
        use mtg_core::basics::Status;
        use mtg_core::ids::ObjId;

        let blocker = ObjId(10);
        let attacker = ObjId(30);
        let aura = ObjId(40);
        let host = ObjId(41);
        let mut v = view();
        let vis = |id: ObjId, ctrl: u32, atts: Vec<ObjId>| ObjView::Visible {
            id,
            chars: CharacteristicsView { grp_id: 1, power: Some(2), toughness: Some(2), ..Default::default() },
            controller: PlayerId(ctrl),
            owner: PlayerId(ctrl),
            zone: mtg_core::basics::Zone::Battlefield,
            status: Status::default(),
            counters: CounterBag::default(),
            damage_marked: 0,
            attachments: atts,
            summoning_sick: false,
        };
        // rows: 0=blocker, 1=attacker, 2=aura, 3=host (host lists the aura as an attachment).
        v.battlefield = vec![vis(blocker, 0, vec![]), vis(attacker, 1, vec![]),
                             vis(aura, 0, vec![]), vis(host, 0, vec![aura])];
        let req = DecisionRequest::DeclareBlockers {
            eligible: vec![BlockerOption { creature: blocker, may_block: vec![attacker], required: false, block_cost: None }],
            attackers: vec![attacker],
        };
        let o = encode(&v, &req, 3,
                       &PendingView { blocks: vec![(blocker, attacker)], ..Default::default() });
        let edges = edge_tuples(&o);
        // The pending block: PENDING_PICK (decision → blocker row 0, k=0) AND BLOCKS (row 0 → row 1).
        assert!(edges.contains(&(ROW_DECISION as i64, 0, EDGE_PENDING_PICK, 0)),
                "pending pick edge from the decision token to the assigned blocker");
        assert!(edges.contains(&(0, 1, EDGE_BLOCKS, 0)),
                "BLOCKS edge appears immediately for the pending assignment");
        // The aura (row 2) attached to its host (row 3).
        assert!(edges.contains(&(2, 3, EDGE_ATTACHED_TO, 0)), "aura → host attachment edge");
        // No raw entityid anywhere in the feature tensor (ids 10/30/40/41 don't appear as columns).
        assert_eq!(F_PERM, BF_PENDING_COMBAT + 1, "no id columns after the combat tail");
    }

    /// v3 §7.4 — stack targeting (gap G1 closed): a spell on the stack emits TARGETS edges to its
    /// object/player targets (k = target order) and STACK_SOURCE back to its source permanent.
    #[test]
    fn edges_expose_stack_targets_and_source() {
        use mtg_core::agent::{CharacteristicsView, StackObjView};
        use mtg_core::basics::{Status, Target};
        use mtg_core::ids::{ObjId, StackId};

        let creature = ObjId(10); // bf row 0
        let source_perm = ObjId(11); // bf row 1
        let mut v = view();
        let vis = |id: ObjId| ObjView::Visible {
            id,
            chars: CharacteristicsView { grp_id: 1, power: Some(2), toughness: Some(2), ..Default::default() },
            controller: PlayerId(0),
            owner: PlayerId(0),
            zone: mtg_core::basics::Zone::Battlefield,
            status: Status::default(),
            counters: CounterBag::default(),
            damage_marked: 0,
            attachments: vec![],
            summoning_sick: false,
        };
        v.battlefield = vec![vis(creature), vis(source_perm)];
        v.stack = vec![StackObjView {
            id: StackId(1),
            controller: PlayerId(0),
            source: Some(source_perm),
            chars: CharacteristicsView { grp_id: 555, ..Default::default() },
            targets: vec![Target::Object(creature), Target::Player(PlayerId(1))],
        }];
        let req = DecisionRequest::Priority { actions: vec![], can_pass: true };
        let o = encode(&v, &req, 1, &PendingView::default());
        let edges = edge_tuples(&o);
        let srow = ROW_STACK as i64; // the spell is stack row 0
        assert!(edges.contains(&(srow, 0, EDGE_TARGETS, 0)), "first target: the creature (k=0)");
        assert!(edges.contains(&(srow, ROW_OPP as i64, EDGE_TARGETS, 1)), "second target: the opponent (k=1)");
        assert!(edges.contains(&(srow, 1, EDGE_STACK_SOURCE, 0)), "stack object → its source permanent");
    }

    /// v3 §7.5 — abstract options become content tokens: choice rows arrive verbatim from the codec
    /// so `NUMBER[j]` / choice row `j` can never disagree about which number slot `j` submits.
    #[test]
    fn choice_feat_carries_codec_choice_rows() {
        use crate::codec::{ChoiceKind, ChoiceRow};
        let req = DecisionRequest::Priority { actions: vec![], can_pass: true };
        let pending = PendingView {
            choices: vec![
                ChoiceRow { row: 0, kind: ChoiceKind::Number, value: 3.0, color: None },
                ChoiceRow { row: 1, kind: ChoiceKind::Number, value: 5.0, color: None },
                ChoiceRow { row: 2, kind: ChoiceKind::Color, value: 2.0, color: Some(2) },
            ],
            ..Default::default()
        };
        let o = encode(&view(), &req, 2, &pending);
        let row = |r: usize, c: usize| o.choice_feat[r * F_CHOICE + c];
        assert_eq!(row(0, 0), 1.0, "row 0 present");
        assert_eq!(row(0, 1 + 2), 1.0, "row 0 kind = number");
        assert_eq!(row(0, 5), 3.0, "row 0 submits 3");
        assert_eq!(row(1, 5), 5.0, "row 1 submits 5");
        assert_eq!(row(2, 1 + 1), 1.0, "row 2 kind = color");
        assert_eq!(row(2, 6 + 2), 1.0, "row 2 color one-hot = Black (WUBRG idx 2)");
        assert_eq!(row(3, 0), 0.0, "row 3 absent");
    }
}
