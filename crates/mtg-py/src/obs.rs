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
//!   candidate of the current decision**) + its `grp_id` (the card-embedding id, separated out for
//!   an embedding lookup in the policy's features extractor);
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
use mtg_core::basics::{Color, Phase};
use mtg_core::ids::ObjId;
use std::collections::BTreeSet;

use crate::layout::{
    CARD_TYPES, COLORS, KEYWORDS, MAX_HAND, MAX_PERM, MAX_STACK, N_CARD_TYPES, N_COLORS, N_KEYWORDS,
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
/// Per-battlefield-row feature width (excludes `grp_id`, which rides in `bf_ids`).
pub const F_PERM: usize = 9 + N_CARD_TYPES + N_COLORS + N_KEYWORDS + 4 + DECISION_FLAGS;
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

/// The structured observation. Flat `Vec`s (Python reshapes per [`spec`]); `*_ids` are the
/// per-row `grp_id`s (0 = empty row) for the policy's embedding table.
#[derive(Debug, Clone)]
pub struct Obs {
    pub globals: Vec<f32>,
    pub bf_feat: Vec<f32>,
    pub bf_ids: Vec<i64>,
    pub hand_feat: Vec<f32>,
    pub hand_ids: Vec<i64>,
    pub stack_feat: Vec<f32>,
    pub stack_ids: Vec<i64>,
    /// The resolved source-card `grp_id` of the current decision (0 = none) — see module docs.
    pub decision_ids: Vec<i64>,
}

/// `(name, rows, cols, is_int)` for each obs array — Python builds the `gym.spaces.Dict` from this
/// (shapes are never hard-coded on the Python side).
pub fn spec() -> Vec<(&'static str, usize, usize, bool)> {
    vec![
        ("globals", 1, G, false),
        ("bf_feat", MAX_PERM, F_PERM, false),
        ("bf_ids", 1, MAX_PERM, true),
        ("hand_feat", MAX_HAND, F_HAND, false),
        ("hand_ids", 1, MAX_HAND, true),
        ("stack_feat", MAX_STACK, F_STACK, false),
        ("stack_ids", 1, MAX_STACK, true),
        // Source-card identity of the current decision (Tier 1) — one row, one grp_id; Python maps
        // it to a one-hot through the same deck-local card index as `*_ids`.
        ("decision_ids", 1, 1, true),
    ]
}

/// Encode `view` + the current request (and its legal-option count) into the structured [`Obs`].
pub fn encode(view: &PlayerView, req: &DecisionRequest, num_legal: usize) -> Obs {
    let di = decision_info(view, req);
    Obs {
        globals: encode_globals(view, req, num_legal, &di),
        bf_feat: encode_battlefield(view, &di),
        bf_ids: ids(&view.battlefield, MAX_PERM),
        hand_feat: encode_hand(view, req, &di),
        hand_ids: ids(&view.me.hand, MAX_HAND),
        stack_feat: encode_stack(view, &di),
        stack_ids: view
            .stack
            .iter()
            .take(MAX_STACK)
            .map(|s| s.chars.grp_id as i64)
            .chain(std::iter::repeat(0))
            .take(MAX_STACK)
            .collect(),
        decision_ids: vec![di.src_grp],
    }
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
    // We locate the source by matching `for_action`'s StackId against the stack. This resolves a
    // SPELL mid-cast (on the stack at 601.2c) and an ACTIVATED ability (pushed before its targets),
    // but NOT a TRIGGERED/reflexive ability: the engine chooses its targets *before* pushing it onto
    // the stack (priority.rs `place_trigger`), so it isn't in `view.stack` yet and its source can't
    // be recovered here. Closing that needs the engine to carry `source` in the request itself —
    // tracked as the Tier-1 engine follow-up; this code will prefer that field once it exists.
    let for_action = match req {
        Q::ChooseTargets { for_action, .. }
        | Q::ChooseModes { for_action, .. }
        | Q::CastingTimeOptions { for_action, .. } => Some(for_action.0),
        _ => None,
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

fn encode_battlefield(view: &PlayerView, di: &DecisionInfo) -> Vec<f32> {
    let me = view.seat;
    let (attacking, blocking) = combat_sets(view);
    let mut out = Vec::with_capacity(MAX_PERM * F_PERM);
    for o in view.battlefield.iter().take(MAX_PERM) {
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
            }
            ObjView::Hidden { .. } => {
                // Hidden permanent (e.g. a face-down): present but featureless.
                out.push(1.0);
                out.extend(std::iter::repeat(0.0).take(F_PERM - 1));
            }
        }
    }
    out.extend(std::iter::repeat(0.0).take((MAX_PERM - view.battlefield.len().min(MAX_PERM)) * F_PERM));
    out
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
        let o = encode(&view(), &req, 1);
        assert_eq!(o.globals.len(), G);
        assert_eq!(o.bf_feat.len(), MAX_PERM * F_PERM);
        assert_eq!(o.bf_ids.len(), MAX_PERM);
        assert_eq!(o.hand_feat.len(), MAX_HAND * F_HAND);
        assert_eq!(o.hand_ids.len(), MAX_HAND);
        assert_eq!(o.stack_feat.len(), MAX_STACK * F_STACK);
        assert_eq!(o.stack_ids.len(), MAX_STACK);
        assert!(o.globals.iter().all(|x| x.is_finite()));
        assert!(o.bf_feat.iter().all(|x| x.is_finite()));
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
        // (Each grew by DECISION_FLAGS=2 for the Tier-1 source/candidate per-row flags.)
        assert_eq!(F_PERM, 43);
        assert_eq!(F_HAND, 18);
        assert_eq!(F_STACK, 18);
        assert_eq!(G, 69);
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
            slots: vec![TargetSlot {
                description: String::new(),
                legal: vec![Target::Object(creature), Target::Player(PlayerId(1))],
                min: 1,
                max: 1,
            }],
        };

        let o = encode(&v, &req, 2);
        // (a) source card identity = the spell's grp_id.
        assert_eq!(o.decision_ids, vec![555], "decision_ids carries the source spell's grp_id");
        // (b) the stack row (row 0) is flagged is_src (the last-but-one feature of F_STACK).
        assert_eq!(o.stack_feat[F_STACK - 2], 1.0, "source spell's stack row flagged is_src");
        // (c) the creature row (row 0) is flagged is_cand (the last feature of F_PERM).
        assert_eq!(o.bf_feat[F_PERM - 1], 1.0, "legal-target creature flagged is_cand");
        assert_eq!(o.bf_feat[F_PERM - 2], 0.0, "the opponent's creature is not the source");
        // and the opponent is a player-candidate (last global), self is not (second-last).
        assert_eq!(o.globals[G - 1], 1.0, "opponent is a legal player target");
        assert_eq!(o.globals[G - 2], 0.0, "self is not a legal target here");
    }
}
