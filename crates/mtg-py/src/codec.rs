//! The milestone-1 **action codec** — the swappable seam projecting the engine's 21 heterogeneous
//! [`DecisionRequest`] variants onto a single fixed-width `Discrete(ACTION_DIM)` head + boolean
//! mask (GYM_PLAN §4.2-A), MaskablePPO-friendly.
//!
//! **Factored vocabulary.** The action index space is partitioned into buckets whose slots are
//! *positional indices into the padded observation* (so an action points at an entity row the
//! policy already saw): `COMMIT`, `HAND[i]`, `PERM[i]`, `PLAYER[i]`, `STACK[i]`, `MODE[i]`,
//! `COLOR[i]`, `NUMBER[i]`, `YES`/`NO`. Row ordering + sizes come from [`crate::layout`], shared
//! with the obs encoder.
//!
//! **Env-side autoregression.** Multi-target / combat / multi-select / ordering decisions are
//! *batched* engine requests, but a flat `Discrete` head can't emit a subset in one shot — so a
//! single engine request is decomposed into a sequence of single-index sub-steps held in an
//! [`Interaction`]: pick the next attacker / target / card, or `COMMIT`. The full
//! [`DecisionResponse`] is assembled only on commit (the engine request stays batched, preserving
//! the 1:1 GRE alignment). Rare/structured value decisions (`Distribute`, `AssignCombatDamage`,
//! `PayCost`, …) start from the engine's canonical default behind a single `COMMIT` (GYM_PLAN §4.2
//! — "start with auto-spread, add a real head when a card makes the split matter").
//!
//! Every enumerated slot is legal by construction and `apply` clamps anything off-mask, so a buggy
//! policy can never make an illegal move or wedge a game.

use std::collections::BTreeSet;

use mtg_core::agent::{DecisionRequest, DecisionResponse, PlayableAction, PlayerView};
use mtg_core::basics::{Color, Target};
use mtg_core::ids::{ObjId, PlayerId, StackId};

use crate::layout::{self, MAX_HAND, MAX_PERM, MAX_STACK, N_COLORS};

// ── flat vocabulary layout (derived from the shared table sizes) ────────────────────────────
pub const MAX_MODES: usize = 16;
pub const MAX_NUM: usize = 16;
const N_PLAYER_SLOTS: usize = 2;

pub const COMMIT: usize = 0;
pub const HAND_BASE: usize = COMMIT + 1;
pub const PERM_BASE: usize = HAND_BASE + MAX_HAND;
pub const PLAYER_BASE: usize = PERM_BASE + MAX_PERM;
pub const STACK_BASE: usize = PLAYER_BASE + N_PLAYER_SLOTS;
pub const MODE_BASE: usize = STACK_BASE + MAX_STACK;
pub const COLOR_BASE: usize = MODE_BASE + MAX_MODES;
pub const NUMBER_BASE: usize = COLOR_BASE + N_COLORS;
pub const YES: usize = NUMBER_BASE + MAX_NUM;
pub const NO: usize = YES + 1;
pub const ACTION_DIM: usize = NO + 1;

fn color_index(c: Color) -> Option<usize> {
    layout::COLORS.iter().position(|&x| x == c)
}

/// Map a candidate to a non-`COMMIT` slot: an unmappable candidate (not in any visible row) falls
/// back to a positional `MODE` slot so it can never collide with the `COMMIT`/commit action (which
/// would risk a wedge). Mappable candidates keep their real `PERM`/`PLAYER`/`STACK`/`HAND` slot.
fn nonzero(slot: usize, fallback_idx: usize) -> usize {
    if slot == COMMIT {
        MODE_BASE + fallback_idx.min(MAX_MODES - 1)
    } else {
        slot
    }
}

/// One in-flight engine decision, decomposed into single-index sub-steps. `apply` returns `Some`
/// once enough sub-steps have committed a full [`DecisionResponse`].
pub struct Interaction {
    view: PlayerView,
    req: DecisionRequest,
    me: PlayerId,
    // Ordered entity ids (index = the bucket's slot index), mirroring the obs row order.
    bf: Vec<ObjId>,
    hand: Vec<ObjId>,
    stack: Vec<StackId>,
    state: IState,
    committed: Option<DecisionResponse>,
}

enum IState {
    /// One pick from a `(slot, response)` table — commits immediately.
    Single(Vec<(usize, DecisionResponse)>),
    /// Accumulate a subset; `slot_of[i]` is option `i`'s vocab slot. Commits `Indices(picked)` on
    /// `COMMIT` (≥ min) or auto at max.
    Subset {
        slot_of: Vec<usize>,
        picked: Vec<u32>,
        min: u32,
        max: u32,
    },
    /// Targets, accumulated per target-slot. `cand_slot[s][c]` = candidate `c` of slot `s`'s vocab
    /// slot. Commits `Pairs((slot, cand))` when every slot has ≥ its min.
    Targets {
        cand_slot: Vec<Vec<usize>>,
        mins: Vec<u32>,
        maxs: Vec<u32>,
        cur: usize,
        current: Vec<u32>,
        done: Vec<Vec<u32>>,
    },
    /// Declare attackers: `atk_slot[i]` = eligible attacker `i`'s perm slot; `def_slot[i][d]` =
    /// defender `d`'s vocab slot. `pending_def` holds the attacker awaiting a defender pick.
    Attackers {
        atk_slot: Vec<usize>,
        def_slot: Vec<Vec<usize>>,
        chosen: Vec<(u32, u32)>,
        pending_def: Option<u32>,
    },
    /// Declare blockers: `blk_slot[i]` = blocker `i`'s perm slot; `atk_slot[i][a]` = the perm slot
    /// of attacker `a` in blocker `i`'s `may_block`.
    Blockers {
        blk_slot: Vec<usize>,
        atk_slot: Vec<Vec<usize>>,
        chosen: Vec<(u32, u32)>,
        pending_atk: Option<u32>,
    },
    /// Order: `item_slot[i]` = item `i`'s vocab slot; commit `Order(placed)` once all placed.
    Order {
        item_slot: Vec<usize>,
        placed: Vec<u32>,
    },
}

impl Interaction {
    pub fn new(view: &PlayerView, req: &DecisionRequest) -> Interaction {
        let bf: Vec<ObjId> = view
            .battlefield
            .iter()
            .take(MAX_PERM)
            .map(layout::objview_id)
            .collect();
        let hand: Vec<ObjId> = view
            .me
            .hand
            .iter()
            .take(MAX_HAND)
            .map(layout::objview_id)
            .collect();
        let stack: Vec<StackId> = view.stack.iter().take(MAX_STACK).map(|s| s.id).collect();
        let mut it = Interaction {
            view: view.clone(),
            req: req.clone(),
            me: view.seat,
            bf,
            hand,
            stack,
            state: IState::Single(vec![]),
            committed: None,
        };
        it.state = it.build_state();
        it
    }

    pub fn view(&self) -> &PlayerView {
        &self.view
    }
    pub fn req(&self) -> &DecisionRequest {
        &self.req
    }

    pub fn num_legal(&self) -> usize {
        self.legal_slots().len()
    }

    /// In-flight DeclareBlockers state for the observation encoder: the `(blocker, attacker)` ObjId
    /// pairs assigned SO FAR this decision (pending, not yet committed) plus the blocker currently
    /// awaiting its attacker pick (the "decision source"). `(empty, None)` for every other state. Lets
    /// the obs show pending gang structure the frozen `Interaction::new` view snapshot cannot — the
    /// signal that makes deliberate double-blocking conditionable. Resolves `chosen`'s
    /// `(blocker_idx, attacker_local_idx)` through `eligible[bi].may_block[ai]` (combat/mod.rs:280).
    pub fn pending_block_view(&self) -> (Vec<(ObjId, ObjId)>, Option<ObjId>) {
        if let (DecisionRequest::DeclareBlockers { eligible, .. },
                IState::Blockers { chosen, pending_atk, .. }) = (&self.req, &self.state)
        {
            let pairs = chosen
                .iter()
                .filter_map(|&(bi, ai)| {
                    let opt = eligible.get(bi as usize)?;
                    Some((opt.creature, *opt.may_block.get(ai as usize)?))
                })
                .collect();
            let source = pending_atk.and_then(|i| eligible.get(i as usize)).map(|o| o.creature);
            (pairs, source)
        } else {
            (Vec::new(), None)
        }
    }

    pub fn mask(&self) -> Vec<bool> {
        let legal = self.legal_slots();
        let set: BTreeSet<usize> = legal.into_iter().collect();
        (0..ACTION_DIM).map(|i| set.contains(&i)).collect()
    }

    // ── slot mapping ────────────────────────────────────────────────────────────────────────
    fn perm_slot(&self, id: ObjId) -> Option<usize> {
        self.bf.iter().position(|&x| x == id).map(|i| PERM_BASE + i)
    }
    fn hand_slot(&self, id: ObjId) -> Option<usize> {
        self.hand.iter().position(|&x| x == id).map(|i| HAND_BASE + i)
    }
    fn stack_slot(&self, id: StackId) -> Option<usize> {
        self.stack
            .iter()
            .position(|&x| x == id)
            .map(|i| STACK_BASE + i)
    }
    fn player_slot(&self, p: PlayerId) -> usize {
        PLAYER_BASE + if p == self.me { 0 } else { 1 }
    }
    /// An object's vocab slot, searching battlefield → stack → hand.
    fn obj_slot(&self, id: ObjId) -> Option<usize> {
        self.perm_slot(id).or_else(|| self.hand_slot(id))
    }
    fn target_slot(&self, t: &Target) -> Option<usize> {
        match t {
            Target::Player(p) => Some(self.player_slot(*p)),
            Target::Object(id) => self.obj_slot(*id),
            Target::Stack(sid) => self.stack_slot(*sid),
        }
    }

    // ── per-request decomposition ─────────────────────────────────────────────────────────────
    fn build_state(&self) -> IState {
        use DecisionRequest as Q;
        match &self.req {
            Q::ChooseStartingPlayer { candidates } => {
                let table = candidates
                    .iter()
                    .enumerate()
                    .map(|(i, p)| (self.player_slot(*p), DecisionResponse::Index(i as u32)))
                    .collect();
                IState::Single(table)
            }
            Q::Mulligan { .. } => IState::Single(vec![
                (NO, DecisionResponse::Bool(false)),  // keep
                (YES, DecisionResponse::Bool(true)),  // mulligan
            ]),
            Q::Priority { actions, can_pass } => {
                let mut table: Vec<(usize, DecisionResponse)> = vec![];
                let mut used: BTreeSet<usize> = BTreeSet::new();
                for (i, a) in actions.iter().enumerate() {
                    let slot = match a {
                        PlayableAction::Cast { spell, .. } => self.obj_slot(*spell),
                        PlayableAction::PlayLand { card } => self.obj_slot(*card),
                        PlayableAction::Activate { source, .. }
                        | PlayableAction::ActivateMana { source, .. } => self.perm_slot(*source),
                        PlayableAction::Special { .. } => None,
                    };
                    if let Some(s) = slot {
                        // First action to claim a slot wins (≤1 relevant action per entity in the
                        // M1 pool; an ABILITY sub-slot disambiguates multiples later).
                        if used.insert(s) {
                            table.push((s, DecisionResponse::Action(i as u32)));
                        }
                    }
                }
                if *can_pass || table.is_empty() {
                    table.push((COMMIT, DecisionResponse::Pass));
                }
                IState::Single(table)
            }
            Q::Confirm { .. } => IState::Single(vec![
                (YES, DecisionResponse::Bool(true)),
                (NO, DecisionResponse::Bool(false)),
            ]),
            Q::ChooseNumber {
                min,
                max,
                step,
                forbidden,
                disallow_even,
                disallow_odd,
                ..
            } => {
                let step = (*step).max(1) as usize;
                let legal: Vec<i64> = (*min..=*max)
                    .step_by(step)
                    .filter(|n| !forbidden.contains(n))
                    .filter(|n| !(*disallow_even && n.rem_euclid(2) == 0))
                    .filter(|n| !(*disallow_odd && n.rem_euclid(2) != 0))
                    .collect();
                let legal = if legal.is_empty() { vec![*min] } else { legal };
                // Bucket evenly into MAX_NUM number slots.
                let table = (0..legal.len().min(MAX_NUM))
                    .map(|j| {
                        let idx = j * legal.len() / legal.len().min(MAX_NUM);
                        (NUMBER_BASE + j, DecisionResponse::Number(legal[idx]))
                    })
                    .collect();
                IState::Single(table)
            }
            Q::ChooseCounterType { options } => IState::Single(
                (0..options.len())
                    .map(|i| (MODE_BASE + i.min(MAX_MODES - 1), DecisionResponse::Index(i as u32)))
                    .collect(),
            ),
            Q::ChooseReplacement { applicable, .. } => IState::Single(
                (0..applicable.len())
                    .map(|i| (MODE_BASE + i.min(MAX_MODES - 1), DecisionResponse::Index(i as u32)))
                    .collect(),
            ),
            Q::SelectCards { from, min, max, .. } => {
                let slot_of = from
                    .iter()
                    .enumerate()
                    .map(|(i, &id)| nonzero(self.obj_slot(id).unwrap_or(COMMIT), i))
                    .collect();
                IState::Subset { slot_of, picked: vec![], min: *min, max: *max }
            }
            Q::ChooseModes { modes, min, max, .. } => {
                let slot_of = (0..modes.len()).map(|i| MODE_BASE + i.min(MAX_MODES - 1)).collect();
                IState::Subset { slot_of, picked: vec![], min: *min, max: *max }
            }
            Q::ChooseOption { options, min, max, .. } => {
                let slot_of = (0..options.len()).map(|i| MODE_BASE + i.min(MAX_MODES - 1)).collect();
                IState::Subset { slot_of, picked: vec![], min: *min, max: *max }
            }
            Q::ChooseColor { allowed, min, max } => {
                let slot_of = allowed
                    .iter()
                    .map(|&c| color_index(c).map(|ci| COLOR_BASE + ci).unwrap_or(COMMIT))
                    .collect();
                IState::Subset { slot_of, picked: vec![], min: *min, max: *max }
            }
            Q::ChooseTargets { slots, .. } => {
                let cand_slot: Vec<Vec<usize>> = slots
                    .iter()
                    .map(|s| {
                        s.legal
                            .iter()
                            .enumerate()
                            .map(|(c, t)| nonzero(self.target_slot(t).unwrap_or(COMMIT), c))
                            .collect()
                    })
                    .collect();
                let mins = slots.iter().map(|s| s.min).collect();
                let maxs = slots.iter().map(|s| s.max).collect();
                IState::Targets {
                    cand_slot,
                    mins,
                    maxs,
                    cur: 0,
                    current: vec![],
                    done: vec![],
                }
            }
            Q::DeclareAttackers { eligible } => {
                let atk_slot = eligible
                    .iter()
                    .map(|e| self.perm_slot(e.creature).unwrap_or(COMMIT))
                    .collect();
                let def_slot = eligible
                    .iter()
                    .map(|e| {
                        e.may_attack
                            .iter()
                            .enumerate()
                            .map(|(d, t)| nonzero(self.target_slot(t).unwrap_or(COMMIT), d))
                            .collect()
                    })
                    .collect();
                // Auto-include required attackers at their first legal defender.
                let chosen = eligible
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.required && !e.may_attack.is_empty())
                    .map(|(i, _)| (i as u32, 0u32))
                    .collect();
                IState::Attackers { atk_slot, def_slot, chosen, pending_def: None }
            }
            Q::DeclareBlockers { eligible, .. } => {
                let blk_slot = eligible
                    .iter()
                    .map(|e| self.perm_slot(e.creature).unwrap_or(COMMIT))
                    .collect();
                let atk_slot = eligible
                    .iter()
                    .map(|e| {
                        e.may_block
                            .iter()
                            .enumerate()
                            .map(|(a, &id)| nonzero(self.perm_slot(id).unwrap_or(COMMIT), a))
                            .collect()
                    })
                    .collect();
                IState::Blockers { blk_slot, atk_slot, chosen: vec![], pending_atk: None }
            }
            Q::OrderObjects { items, .. } => {
                let item_slot = items
                    .iter()
                    .enumerate()
                    .map(|(i, &id)| self.perm_slot(id).unwrap_or(MODE_BASE + i.min(MAX_MODES - 1)))
                    .collect();
                IState::Order { item_slot, placed: vec![] }
            }
            // ── rare / value-bearing: canonical default behind a single COMMIT (GYM_PLAN §4.2) ──
            Q::Distribute { among, total, min_each, .. } => {
                IState::Single(vec![(COMMIT, DecisionResponse::Amounts(auto_spread(among.len(), *total, *min_each)))])
            }
            Q::AssignCombatDamage { total, .. } => {
                IState::Single(vec![(COMMIT, DecisionResponse::Amounts(vec![(0, *total)]))])
            }
            Q::PayCost { mana_sources, .. } => IState::Single(vec![(
                COMMIT,
                DecisionResponse::Payment { mana: (0..mana_sources.len() as u32).collect(), non_mana: vec![] },
            )]),
            Q::CastingTimeOptions { options, .. } => {
                let required: Vec<u32> = options
                    .iter()
                    .enumerate()
                    .filter(|(_, o)| o.required)
                    .map(|(i, _)| i as u32)
                    .collect();
                IState::Single(vec![(COMMIT, DecisionResponse::Indices(required))])
            }
            Q::SelectFromGroups { groups, .. } => {
                let pairs = groups
                    .iter()
                    .enumerate()
                    .flat_map(|(g, grp)| (0..grp.min.min(grp.options.len() as u32)).map(move |c| (g as u32, c)))
                    .collect();
                IState::Single(vec![(COMMIT, DecisionResponse::Pairs(pairs))])
            }
            Q::ArrangeCards { cards, .. } => IState::Single(vec![(
                COMMIT,
                DecisionResponse::Arrangement((0..cards.len() as u32).map(|i| (i, 0, i)).collect()),
            )]),
        }
    }

    // ── legality (current sub-step) ───────────────────────────────────────────────────────────
    fn legal_slots(&self) -> Vec<usize> {
        if self.committed.is_some() {
            return vec![];
        }
        match &self.state {
            IState::Single(table) => {
                if table.is_empty() {
                    vec![COMMIT]
                } else {
                    table.iter().map(|(s, _)| *s).collect()
                }
            }
            IState::Subset { slot_of, picked, min, max } => {
                let mut v = vec![];
                if picked.len() as u32 >= *min {
                    v.push(COMMIT);
                }
                if (picked.len() as u32) < *max {
                    for (i, &s) in slot_of.iter().enumerate() {
                        if !picked.contains(&(i as u32)) {
                            v.push(s);
                        }
                    }
                }
                if v.is_empty() {
                    v.push(COMMIT);
                }
                v
            }
            IState::Targets { cand_slot, mins, maxs, cur, current, .. } => {
                let mut v = vec![];
                let min = mins.get(*cur).copied().unwrap_or(0);
                let max = maxs.get(*cur).copied().unwrap_or(0);
                if current.len() as u32 >= min {
                    v.push(COMMIT); // commit this slot (advance / finish)
                }
                if (current.len() as u32) < max {
                    if let Some(cands) = cand_slot.get(*cur) {
                        for (c, &s) in cands.iter().enumerate() {
                            if !current.contains(&(c as u32)) {
                                v.push(s);
                            }
                        }
                    }
                }
                if v.is_empty() {
                    v.push(COMMIT);
                }
                v
            }
            IState::Attackers { atk_slot, def_slot, chosen, pending_def } => {
                if let Some(i) = pending_def {
                    return def_slot[*i as usize].clone();
                }
                let mut v = vec![COMMIT];
                for (i, &s) in atk_slot.iter().enumerate() {
                    if !chosen.iter().any(|(a, _)| *a == i as u32) && !def_slot[i].is_empty() {
                        v.push(s);
                    }
                }
                v
            }
            IState::Blockers { blk_slot, atk_slot, chosen, pending_atk } => {
                if let Some(i) = pending_atk {
                    return atk_slot[*i as usize].clone();
                }
                let mut v = vec![COMMIT];
                for (i, &s) in blk_slot.iter().enumerate() {
                    if !chosen.iter().any(|(b, _)| *b == i as u32) && !atk_slot[i].is_empty() {
                        v.push(s);
                    }
                }
                v
            }
            IState::Order { item_slot, placed } => {
                let mut v: Vec<usize> = item_slot
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !placed.contains(&(*i as u32)))
                    .map(|(_, &s)| s)
                    .collect();
                if v.is_empty() {
                    v.push(COMMIT);
                }
                v
            }
        }
    }

    // ── apply one sub-step action; returns Some(response) when the decision is complete ────────
    pub fn apply(&mut self, action: usize) -> Option<DecisionResponse> {
        if let Some(r) = &self.committed {
            return Some(r.clone());
        }
        // Clamp off-mask actions to the first legal slot (defensive — masked policies never hit it).
        let legal = self.legal_slots();
        let action = if legal.contains(&action) {
            action
        } else {
            legal[0]
        };
        let resp = self.step(action);
        if let Some(r) = &resp {
            self.committed = Some(r.clone());
        }
        resp
    }

    fn step(&mut self, action: usize) -> Option<DecisionResponse> {
        match &mut self.state {
            IState::Single(table) => {
                if table.is_empty() {
                    return Some(DecisionResponse::Pass);
                }
                let r = table
                    .iter()
                    .find(|(s, _)| *s == action)
                    .map(|(_, r)| r.clone())
                    .unwrap_or_else(|| table[0].1.clone());
                Some(r)
            }
            IState::Subset { slot_of, picked, min: _, max } => {
                // COMMIT finalizes the subset. The mask only offers COMMIT once `picked.len() >= min`
                // OR when stuck (min required but no legal option left to add), so committing here is
                // always intended — guarding on `>= min` would spin forever in the stuck case (same
                // class of hang as IState::Targets).
                if action == COMMIT {
                    return Some(DecisionResponse::Indices(picked.clone()));
                }
                // Add the first not-yet-picked option whose slot matches.
                if let Some(i) = slot_of
                    .iter()
                    .enumerate()
                    .find(|(i, &s)| s == action && !picked.contains(&(*i as u32)))
                    .map(|(i, _)| i as u32)
                {
                    picked.push(i);
                }
                if picked.len() as u32 >= *max {
                    return Some(DecisionResponse::Indices(picked.clone()));
                }
                None
            }
            IState::Targets { cand_slot, maxs, cur, current, done, .. } => {
                let max = maxs.get(*cur).copied().unwrap_or(0);
                // COMMIT finalizes this target slot with whatever's chosen. The mask only offers
                // COMMIT once `current.len() >= min` OR when the slot is *stuck* — `min` targets are
                // required but no legal candidate maps to a slot, so `legal_slots` falls back to
                // COMMIT. We MUST advance in the stuck case too: guarding on `>= min` here would
                // re-present the same slot forever (a real selesnya self-play hang). Best-effort
                // (possibly < min) is the robust choice; an unsatisfiable required slot is an engine
                // bug, flagged separately — the codec must never wedge regardless.
                if action == COMMIT {
                    done.push(std::mem::take(current));
                    *cur += 1;
                } else if let Some(cands) = cand_slot.get(*cur) {
                    if let Some(c) = cands.iter().position(|&s| s == action) {
                        if !current.contains(&(c as u32)) {
                            current.push(c as u32);
                        }
                        if current.len() as u32 >= max {
                            done.push(std::mem::take(current));
                            *cur += 1;
                        }
                    }
                }
                if *cur >= cand_slot.len() {
                    let pairs = done
                        .iter()
                        .enumerate()
                        .flat_map(|(s, picks)| picks.iter().map(move |&c| (s as u32, c)))
                        .collect();
                    return Some(DecisionResponse::Pairs(pairs));
                }
                None
            }
            IState::Attackers { atk_slot, def_slot, chosen, pending_def } => {
                if let Some(i) = *pending_def {
                    let d = def_slot[i as usize].iter().position(|&s| s == action).unwrap_or(0);
                    chosen.push((i, d as u32));
                    *pending_def = None;
                    return None;
                }
                if action == COMMIT {
                    return Some(DecisionResponse::Pairs(chosen.clone()));
                }
                // action is an attacker's perm slot: add it (choose a defender if it has >1).
                if let Some(i) = atk_slot.iter().position(|&s| s == action) {
                    if chosen.iter().any(|(a, _)| *a == i as u32) {
                        return None;
                    }
                    if def_slot[i].len() <= 1 {
                        chosen.push((i as u32, 0));
                    } else {
                        *pending_def = Some(i as u32);
                    }
                }
                None
            }
            IState::Blockers { blk_slot, atk_slot, chosen, pending_atk } => {
                if let Some(i) = *pending_atk {
                    let a = atk_slot[i as usize].iter().position(|&s| s == action).unwrap_or(0);
                    chosen.push((i, a as u32));
                    *pending_atk = None;
                    return None;
                }
                if action == COMMIT {
                    return Some(DecisionResponse::Pairs(chosen.clone()));
                }
                // action is a blocker's perm slot: add it (choose which attacker if it may block >1).
                if let Some(i) = blk_slot.iter().position(|&s| s == action) {
                    if chosen.iter().any(|(b, _)| *b == i as u32) {
                        return None;
                    }
                    if atk_slot[i].len() <= 1 {
                        chosen.push((i as u32, 0));
                    } else {
                        *pending_atk = Some(i as u32);
                    }
                }
                None
            }
            IState::Order { item_slot, placed } => {
                if let Some(i) = item_slot.iter().position(|&s| s == action) {
                    if !placed.contains(&(i as u32)) {
                        placed.push(i as u32);
                    }
                }
                if placed.len() >= item_slot.len() {
                    return Some(DecisionResponse::Order(placed.clone()));
                }
                None
            }
        }
    }
}

/// Auto-spread `total` over `n` recipients (each ≥ `min_each`, remainder on the first).
fn auto_spread(n: usize, total: u32, min_each: u32) -> Vec<(u32, u32)> {
    if n == 0 {
        return vec![];
    }
    let mut a: Vec<(u32, u32)> = (0..n as u32).map(|i| (i, min_each)).collect();
    a[0].1 += total.saturating_sub(min_each * n as u32);
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;
    use mtg_core::agent::*;
    use mtg_core::ids::{ObjId, PlayerId, StackId};

    fn base_view() -> PlayerView {
        PlayerView {
            seat: PlayerId(0),
            turn: 1,
            active_player: PlayerId(0),
            phase: mtg_core::basics::Phase::PrecombatMain,
            priority_player: Some(PlayerId(0)),
            players: vec![],
            me: PlayerPrivateView { hand: vec![], known_library: vec![], revealed_to_me: vec![] },
            battlefield: vec![],
            stack: vec![],
            combat: None,
            stops: None,
        }
    }

    fn vis(id: u64) -> ObjView {
        ObjView::Visible {
            id: ObjId(id),
            chars: CharacteristicsView::default(),
            controller: PlayerId(0),
            owner: PlayerId(0),
            zone: mtg_core::basics::Zone::Battlefield,
            status: mtg_core::basics::Status::default(),
            counters: mtg_core::basics::CounterBag::default(),
            damage_marked: 0,
            attachments: vec![],
            summoning_sick: false,
        }
    }

    #[test]
    fn confirm_is_yes_no() {
        let mut it =
            Interaction::new(&base_view(), &DecisionRequest::Confirm { kind: ConfirmKind::MayEffect });
        let m = it.mask();
        assert_eq!(m.iter().filter(|b| **b).count(), 2);
        assert!(m[YES] && m[NO]);
        assert_eq!(it.apply(YES), Some(DecisionResponse::Bool(true)));
    }

    #[test]
    fn single_target_picks_a_candidate_in_one_substep() {
        let mut view = base_view();
        view.battlefield = vec![vis(10), vis(11)];
        let req = DecisionRequest::ChooseTargets {
            for_action: ActionRef(StackId(1)),
            source: None,
            slots: vec![TargetSlot {
                description: "t".into(),
                legal: vec![Target::Object(ObjId(10)), Target::Object(ObjId(11))],
                min: 1,
                max: 1,
            }],
        };
        let mut it = Interaction::new(&view, &req);
        // Two candidate slots legal (the two battlefield perms), COMMIT not yet (min=1).
        let mask = it.mask();
        assert_eq!(mask.iter().filter(|b| **b).count(), 2);
        assert!(mask[PERM_BASE] && mask[PERM_BASE + 1]);
        // Pick the second candidate → Pairs([(slot 0, cand 1)]).
        let r = it.apply(PERM_BASE + 1);
        assert_eq!(r, Some(DecisionResponse::Pairs(vec![(0, 1)])));
    }

    #[test]
    fn declare_attackers_autoregressive_then_commit() {
        let mut view = base_view();
        view.battlefield = vec![vis(5), vis(6)];
        let req = DecisionRequest::DeclareAttackers {
            eligible: vec![
                AttackerOption { creature: ObjId(5), may_attack: vec![Target::Player(PlayerId(1))], required: false, attack_cost: None, may_exert: false, may_enlist: false },
                AttackerOption { creature: ObjId(6), may_attack: vec![Target::Player(PlayerId(1))], required: false, attack_cost: None, may_exert: false, may_enlist: false },
            ],
        };
        let mut it = Interaction::new(&view, &req);
        // COMMIT + two attacker perm slots legal.
        assert!(it.mask()[COMMIT] && it.mask()[PERM_BASE] && it.mask()[PERM_BASE + 1]);
        // Add attacker 0 (single defender → no defender sub-step), then commit.
        assert_eq!(it.apply(PERM_BASE), None);
        let r = it.apply(COMMIT);
        assert_eq!(r, Some(DecisionResponse::Pairs(vec![(0, 0)])));
    }

    #[test]
    fn select_cards_subset_accumulates() {
        let mut view = base_view();
        view.me.hand = vec![vis(1), vis(2), vis(3)];
        let req = DecisionRequest::SelectCards {
            reason: SelectReason::Discard,
            from: vec![ObjId(1), ObjId(2), ObjId(3)],
            min: 2,
            max: 2,
            description: "discard two".into(),
        };
        let mut it = Interaction::new(&view, &req);
        // min=2 so COMMIT not legal yet; three hand slots legal.
        assert!(!it.mask()[COMMIT]);
        assert_eq!(it.apply(HAND_BASE), None);
        let r = it.apply(HAND_BASE + 2); // reaches max=2 → auto-commit
        assert_eq!(r, Some(DecisionResponse::Indices(vec![0, 2])));
    }

    // Regression: a required target slot (min≥1) with NO legal candidates must COMMIT best-effort,
    // not loop forever. This is the real Selesnya self-play hang — the mask falls back to COMMIT but
    // the old code only advanced when `current.len() >= min`, re-presenting the slot indefinitely.
    #[test]
    fn unsatisfiable_target_slot_commits_instead_of_looping() {
        let req = DecisionRequest::ChooseTargets {
            for_action: ActionRef(StackId(1)),
            source: None,
            slots: vec![TargetSlot {
                description: "needs a target but none legal".into(),
                legal: vec![], // min≥1 with zero legal candidates → the stuck case
                min: 1,
                max: 1,
            }],
        };
        let mut it = Interaction::new(&base_view(), &req);
        let mask = it.mask();
        assert_eq!(mask.iter().filter(|b| **b).count(), 1, "only COMMIT is legal");
        assert!(mask[COMMIT]);
        // ONE COMMIT must finalize (best-effort: no targets), not return None forever.
        assert_eq!(it.apply(COMMIT), Some(DecisionResponse::Pairs(vec![])));
    }

    // Regression: a subset with min greater than the number of selectable options must also COMMIT
    // best-effort rather than spin (same bug class as the target slot above).
    #[test]
    fn unsatisfiable_subset_commits_instead_of_looping() {
        let mut view = base_view();
        view.me.hand = vec![vis(1)];
        let req = DecisionRequest::SelectCards {
            reason: SelectReason::Discard,
            from: vec![ObjId(1)],
            min: 2, // want 2 but only 1 selectable
            max: 2,
            description: "discard two (only one available)".into(),
        };
        let mut it = Interaction::new(&view, &req);
        assert_eq!(it.apply(HAND_BASE), None); // pick the one available
        // now stuck: min=2 unmet, nothing left to add → mask falls back to COMMIT
        let mask = it.mask();
        assert!(mask[COMMIT] && mask.iter().filter(|b| **b).count() == 1);
        assert_eq!(it.apply(COMMIT), Some(DecisionResponse::Indices(vec![0])));
    }

    #[test]
    fn action_dim_layout_is_pinned() {
        // A change to the table sizes shifts every downstream slot — pin the totals so an
        // accidental obs↔codec desync is caught.
        expect![[r#"
            COMMIT=0 HAND=1 PERM=17 PLAYER=49 STACK=51 MODE=59 COLOR=75 NUMBER=80 YES=96 NO=97 DIM=98
        "#]]
        .assert_eq(&format!(
            "COMMIT={COMMIT} HAND={HAND_BASE} PERM={PERM_BASE} PLAYER={PLAYER_BASE} STACK={STACK_BASE} MODE={MODE_BASE} COLOR={COLOR_BASE} NUMBER={NUMBER_BASE} YES={YES} NO={NO} DIM={ACTION_DIM}\n"
        ));
    }
}
