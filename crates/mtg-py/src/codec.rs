//! The milestone-0 **action codec** — the swappable seam between the engine's heterogeneous
//! [`DecisionRequest`] set and a flat, fixed-width RL action vocabulary.
//!
//! It projects every request into a *non-empty, canonical list of legal* [`DecisionResponse`]s
//! ([`legal_options`]). The Gym side then sees a constant-width `Discrete(ACTION_DIM)` head with
//! a boolean mask (`mask[i] = i < legal_options.len()`); decoding an action index is just
//! indexing back into that list. Because every enumerated option is legal *by construction* and
//! the engine additionally clamps out-of-range responses, a policy can never make an illegal move
//! or wedge a game (GYM_PLAN §4.2, law #2).
//!
//! This is deliberately a **small, canonical** legal set per request, NOT the full combinatorial
//! action space — the factored/autoregressive vocabulary (GYM_PLAN §4.2-A/4.3) is milestone 1.
//! Keeping it behind this one module is the whole point: M1 replaces the projection without
//! touching the `PyGame` thread/channel plumbing or the Python env.
//!
//! Mirrors the *shape* logic of `crates/mtg-gre-server/src/options.rs` (which collapses the same
//! 21 variants into five answer modes) — but as a fixed-width tensor vocabulary rather than a
//! socket projection, and with no dependency on that crate (this crate depends only on `mtg-core`).

use mtg_core::agent::{DecisionRequest, DecisionResponse};

/// Fixed width of the flat action vocabulary `A` for milestone 0. Small for the tiny card pool;
/// the codec caps each request's option list to this and the mask is `ACTION_DIM`-wide. Grow (or
/// replace with the factored vocabulary) in milestone 1.
pub const ACTION_DIM: usize = 64;

/// The non-empty, deduped, capped list of legal [`DecisionResponse`]s for `req`. `mask` and the
/// decode step both derive from this single enumeration, so they can never disagree.
pub fn legal_options(req: &DecisionRequest) -> Vec<DecisionResponse> {
    let mut out = enumerate(req);
    // Every decision must surface at least one legal option (GYM_PLAN exit criterion). A pass is
    // the universal safe fallback if a variant somehow enumerated nothing.
    if out.is_empty() {
        out.push(DecisionResponse::Pass);
    }
    // Drop exact duplicates, preserving order (some variants emit overlapping canonical picks).
    let mut seen: Vec<DecisionResponse> = Vec::new();
    out.retain(|r| {
        if seen.contains(r) {
            false
        } else {
            seen.push(r.clone());
            true
        }
    });
    out.truncate(ACTION_DIM);
    out
}

/// The constant-width legality mask for a request whose option list is `opts`: the first
/// `opts.len()` entries are `true`, the rest `false`.
pub fn mask_from_options(opts: &[DecisionResponse]) -> Vec<bool> {
    (0..ACTION_DIM).map(|i| i < opts.len()).collect()
}

/// Decode a flat action index into the chosen response, clamping out-of-range indices to the
/// last legal option (defensive — mirrors the engine's own clamping; a buggy policy is harmless).
/// `opts` must be the same list [`legal_options`] produced for this request.
pub fn decode(opts: &[DecisionResponse], action: usize) -> DecisionResponse {
    let i = action.min(opts.len().saturating_sub(1));
    opts[i].clone()
}

// ── per-variant enumeration ─────────────────────────────────────────────────────────────────

fn enumerate(req: &DecisionRequest) -> Vec<DecisionResponse> {
    use DecisionRequest as Q;
    use DecisionResponse as R;
    match req {
        Q::ChooseStartingPlayer { candidates } => {
            (0..candidates.len() as u32).map(R::Index).collect()
        }
        // Keep or mulligan — both legal; a real policy decides, a random one explores both.
        Q::Mulligan { .. } => vec![R::Bool(false), R::Bool(true)],
        Q::Priority { actions, can_pass } => {
            let mut v: Vec<R> = (0..actions.len() as u32).map(R::Action).collect();
            if *can_pass || v.is_empty() {
                v.push(R::Pass);
            }
            v
        }
        Q::ChooseModes { modes, min, max, .. } => subset_options(modes.len(), *min, *max)
            .into_iter()
            .map(R::Indices)
            .collect(),
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
            let mut v: Vec<R> = (*min..=*max)
                .step_by(step)
                .filter(|n| !forbidden.contains(n))
                .filter(|n| !(*disallow_even && n.rem_euclid(2) == 0))
                .filter(|n| !(*disallow_odd && n.rem_euclid(2) != 0))
                .map(R::Number)
                .collect();
            if v.is_empty() {
                v.push(R::Number(*min));
            }
            v
        }
        // Decline all optional costs (always legal); also offer paying all *required* ones.
        Q::CastingTimeOptions { options, .. } => {
            let mut v = vec![R::Indices(vec![])];
            let required: Vec<u32> = options
                .iter()
                .enumerate()
                .filter(|(_, o)| o.required)
                .map(|(i, _)| i as u32)
                .collect();
            if !required.is_empty() {
                v.push(R::Indices(required));
            }
            v
        }
        Q::ChooseTargets { slots, .. } => target_options(slots),
        Q::Distribute {
            among,
            total,
            min_each,
            ..
        } => vec![R::Amounts(auto_spread(among.len(), *total, *min_each))],
        Q::PayCost { mana_sources, .. } => vec![R::Payment {
            mana: (0..mana_sources.len() as u32).collect(),
            non_mana: vec![],
        }],
        Q::DeclareAttackers { eligible } => {
            let any_required = eligible.iter().any(|e| e.required);
            // Attack with every eligible creature at its first legal defender (covers required).
            let all: Vec<(u32, u32)> = eligible
                .iter()
                .enumerate()
                .filter(|(_, e)| !e.may_attack.is_empty())
                .map(|(i, _)| (i as u32, 0u32))
                .collect();
            let mut out = Vec::new();
            if !any_required {
                out.push(R::Pairs(vec![])); // declare no attackers
            }
            out.push(R::Pairs(all));
            out
        }
        Q::DeclareBlockers { eligible, .. } => {
            // Each eligible blocker assigned to its first legal attacker; or block nothing.
            let all: Vec<(u32, u32)> = eligible
                .iter()
                .enumerate()
                .filter(|(_, e)| !e.may_block.is_empty())
                .map(|(i, _)| (i as u32, 0u32))
                .collect();
            vec![R::Pairs(vec![]), R::Pairs(all)]
        }
        Q::AssignCombatDamage { total, .. } => vec![R::Amounts(vec![(0, *total)])],
        Q::OrderObjects { items, .. } => vec![R::Order((0..items.len() as u32).collect())],
        Q::SelectCards {
            from, min, max, ..
        } => subset_options(from.len(), *min, *max)
            .into_iter()
            .map(R::Indices)
            .collect(),
        Q::SelectFromGroups { groups, .. } => {
            let pairs: Vec<(u32, u32)> = groups
                .iter()
                .enumerate()
                .flat_map(|(g, grp)| {
                    prefix(grp.options.len(), grp.min).map(move |c| (g as u32, c))
                })
                .collect();
            vec![R::Pairs(pairs)]
        }
        Q::ArrangeCards { cards, .. } => {
            vec![R::Arrangement((0..cards.len() as u32).map(|i| (i, 0, i)).collect())]
        }
        Q::ChooseReplacement { applicable, .. } => {
            (0..applicable.len() as u32).map(R::Index).collect()
        }
        Q::ChooseCounterType { options } => (0..options.len() as u32).map(R::Index).collect(),
        Q::ChooseOption {
            options, min, max, ..
        } => subset_options(options.len(), *min, *max)
            .into_iter()
            .map(R::Indices)
            .collect(),
        Q::ChooseColor { allowed, min, max } => subset_options(allowed.len(), *min, *max)
            .into_iter()
            .map(R::Indices)
            .collect(),
        Q::Confirm { .. } => vec![R::Bool(false), R::Bool(true)],
    }
}

// ── helpers ─────────────────────────────────────────────────────────────────────────────────

/// The first `k` indices of an `n`-element list (clamped to `n`).
fn prefix(n: usize, k: u32) -> impl Iterator<Item = u32> {
    0..(k as usize).min(n) as u32
}

/// Canonical legal subsets of an `n`-item set under a `min..=max` size constraint: the `min`- and
/// `max`-prefixes, plus every single-element pick when a size-1 selection is legal (so the policy
/// can actually *vary* single-mode / single-color / single-target picks — the common case).
fn subset_options(n: usize, min: u32, max: u32) -> Vec<Vec<u32>> {
    let maxc = (max as usize).min(n) as u32;
    let minc = min.min(maxc);
    let mut out: Vec<Vec<u32>> = vec![prefix(n, minc).collect()];
    if minc <= 1 && maxc >= 1 {
        for i in 0..n as u32 {
            out.push(vec![i]);
        }
    }
    out.push(prefix(n, maxc).collect());
    out.sort();
    out.dedup();
    out
}

/// Target picks: the canonical "first `min` candidates per slot", but when there is exactly one
/// single-target slot (the dominant removal/aura case) enumerate each candidate so the policy
/// chooses *which* object to target.
fn target_options(slots: &[mtg_core::agent::TargetSlot]) -> Vec<DecisionResponse> {
    use DecisionResponse as R;
    if slots.len() == 1 && slots[0].min == 1 && slots[0].max == 1 {
        let mut out: Vec<R> = (0..slots[0].legal.len() as u32)
            .map(|c| R::Pairs(vec![(0, c)]))
            .collect();
        if out.is_empty() {
            out.push(R::Pairs(vec![]));
        }
        return out;
    }
    let canonical: Vec<(u32, u32)> = slots
        .iter()
        .enumerate()
        .flat_map(|(s, slot)| prefix(slot.legal.len(), slot.min).map(move |c| (s as u32, c)))
        .collect();
    vec![R::Pairs(canonical)]
}

/// Auto-spread `total` over `n` recipients, each ≥ `min_each`, remainder onto the first (the same
/// default `RandomAgent` / `options.rs` use for `Distribute`/`AssignCombatDamage`).
fn auto_spread(n: usize, total: u32, min_each: u32) -> Vec<(u32, u32)> {
    if n == 0 {
        return vec![];
    }
    let mut amounts: Vec<(u32, u32)> = (0..n as u32).map(|i| (i, min_each)).collect();
    let assigned = min_each * (n as u32);
    amounts[0].1 += total.saturating_sub(assigned);
    amounts
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;
    use mtg_core::agent::*;
    use mtg_core::basics::{Color, CounterKind, Target, Zone};
    use mtg_core::ids::{ObjId, PlayerId, StackId};

    // A representative request of every variant, to assert the codec yields a non-empty, capped,
    // in-range option list for each (the milestone-0 mask invariant) and a legal decode.
    fn one_of_each() -> Vec<DecisionRequest> {
        use DecisionRequest::*;
        vec![
            ChooseStartingPlayer { candidates: vec![PlayerId(0), PlayerId(1)] },
            Mulligan { hand: vec![ObjId(1)], mulligans_taken: 0, will_bottom_if_kept: 0 },
            Priority {
                actions: vec![PlayableAction::PlayLand { card: ObjId(1) }],
                can_pass: true,
            },
            ChooseModes {
                for_action: ActionRef(StackId(1)),
                modes: vec![ModeOption { label: "a".into() }, ModeOption { label: "b".into() }],
                min: 1,
                max: 1,
                allow_repeat: false,
            },
            ChooseNumber {
                reason: NumberReason::ChooseX,
                min: 0,
                max: 5,
                step: 1,
                forbidden: vec![],
                disallow_even: false,
                disallow_odd: false,
            },
            CastingTimeOptions {
                for_action: ActionRef(StackId(1)),
                options: vec![CastOption { label: "kick".into(), required: false }],
            },
            ChooseTargets {
                for_action: ActionRef(StackId(1)),
                slots: vec![TargetSlot {
                    description: "target creature".into(),
                    legal: vec![Target::Object(ObjId(10)), Target::Object(ObjId(11))],
                    min: 1,
                    max: 1,
                }],
            },
            Distribute {
                reason: DistributeReason::DamageEffect,
                among: vec![Target::Player(PlayerId(0)), Target::Player(PlayerId(1))],
                total: 3,
                min_each: 0,
                max_each: None,
            },
            PayCost {
                cost: CostRequest { mana: None, components: vec![] },
                mana_sources: vec![],
                non_mana: vec![],
            },
            DeclareAttackers {
                eligible: vec![AttackerOption {
                    creature: ObjId(5),
                    may_attack: vec![Target::Player(PlayerId(1))],
                    required: false,
                    attack_cost: None,
                    may_exert: false,
                    may_enlist: false,
                }],
            },
            DeclareBlockers {
                eligible: vec![BlockerOption {
                    creature: ObjId(6),
                    may_block: vec![ObjId(5)],
                    required: false,
                    block_cost: None,
                }],
                attackers: vec![ObjId(5)],
            },
            AssignCombatDamage {
                source: ObjId(5),
                recipients: vec![DamageSlot { recipient: Target::Object(ObjId(6)), lethal: 2 }],
                total: 2,
                deathtouch: false,
                trample_to: None,
            },
            OrderObjects { kind: OrderKind::MoveToZone(Zone::Graveyard), items: vec![ObjId(1), ObjId(2)] },
            SelectCards {
                reason: SelectReason::Discard,
                from: vec![ObjId(1), ObjId(2), ObjId(3)],
                min: 1,
                max: 1,
                description: "discard a card".into(),
            },
            SelectFromGroups {
                reason: SelectReason::Generic,
                groups: vec![SelectGroup { label: "g".into(), options: vec![ObjId(1)], min: 0, max: 1 }],
            },
            ArrangeCards {
                reason: ArrangeReason::Scry,
                cards: vec![ObjId(1), ObjId(2)],
                destinations: vec![],
            },
            ChooseReplacement { event: "dmg".into(), applicable: vec![ReplacementOption { source: ObjId(1), description: "x".into() }] },
            ChooseCounterType { options: vec![CounterKind::PlusOnePlusOne] },
            ChooseOption {
                reason: OptionReason::ChooseType,
                options: vec![OptionLabel { label: "Goblin".into() }, OptionLabel { label: "Elf".into() }],
                min: 1,
                max: 1,
            },
            ChooseColor { allowed: vec![Color::Red, Color::Blue], min: 1, max: 1 },
            Confirm { kind: ConfirmKind::MayEffect },
        ]
    }

    #[test]
    fn every_variant_yields_a_nonempty_capped_legal_mask() {
        for req in one_of_each() {
            let opts = legal_options(&req);
            assert!(!opts.is_empty(), "empty option set for {req:?}");
            assert!(opts.len() <= ACTION_DIM, "exceeds ACTION_DIM for {req:?}");
            let mask = mask_from_options(&opts);
            assert_eq!(mask.len(), ACTION_DIM);
            assert_eq!(mask.iter().filter(|b| **b).count(), opts.len());
            // Every legal index decodes to the matching option; out-of-range clamps in-range.
            for i in 0..opts.len() {
                assert_eq!(decode(&opts, i), opts[i]);
            }
            assert_eq!(decode(&opts, ACTION_DIM + 99), opts[opts.len() - 1]);
        }
    }

    #[test]
    fn single_target_enumerates_each_candidate() {
        let req = DecisionRequest::ChooseTargets {
            for_action: ActionRef(StackId(1)),
            slots: vec![TargetSlot {
                description: "t".into(),
                legal: vec![Target::Object(ObjId(10)), Target::Object(ObjId(11)), Target::Player(PlayerId(1))],
                min: 1,
                max: 1,
            }],
        };
        let opts = legal_options(&req);
        expect![[r#"
            [
                Pairs(
                    [
                        (
                            0,
                            0,
                        ),
                    ],
                ),
                Pairs(
                    [
                        (
                            0,
                            1,
                        ),
                    ],
                ),
                Pairs(
                    [
                        (
                            0,
                            2,
                        ),
                    ],
                ),
            ]
        "#]]
        .assert_eq(&format!("{opts:#?}\n"));
    }
}
