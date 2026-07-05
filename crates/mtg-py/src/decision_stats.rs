//! Per-decision **semantic summary** for tracked-stats telemetry (gym task #68).
//!
//! When a factored interaction finalizes, [`summarize`] turns the original [`DecisionRequest`] (the
//! full *eligible/opportunity* set) plus the assembled [`DecisionResponse`] (what was actually
//! *taken*) into a flat `name → value` record. The Python `tracked_stats` registry composes ratios
//! from these fields (e.g. `attack_rate = attack_declared / attack_eligible`), so adding a new stat
//! that reuses existing fields is one Python entry; only a genuinely new measurement adds a field
//! here. Kept card-agnostic and data-only (no PyO3) so it unit-tests in pure Rust.

use mtg_core::agent::{DecisionRequest, DecisionResponse, PlayableAction};

/// Flatten one finalized decision into `(field, value)` opportunity/taken counters. Empty for
/// decision kinds no stat currently measures (the caller simply records nothing).
pub fn summarize(req: &DecisionRequest, resp: &DecisionResponse) -> Vec<(&'static str, f64)> {
    use DecisionRequest as Q;
    let b = |x: bool| x as u8 as f64;
    match req {
        // A priority window: which action *kinds* were legal (opportunities) and which was taken.
        // `cast_rate = cast_taken / cast_legal` (fraction of cast-opportunities taken), and similarly
        // for land drops / activations — all read off the legal `PlayableAction`s, card-agnostic.
        Q::Priority { actions, .. } => {
            let any = |f: fn(&PlayableAction) -> bool| actions.iter().any(f);
            let chosen = match resp {
                DecisionResponse::Action(i) => actions.get(*i as usize),
                _ => None, // Pass (or any non-Action response) = took nothing
            };
            let is = |p: Option<&PlayableAction>, f: fn(&PlayableAction) -> bool| p.is_some_and(f);
            let cast = |a: &PlayableAction| matches!(a, PlayableAction::Cast { .. });
            let land = |a: &PlayableAction| matches!(a, PlayableAction::PlayLand { .. });
            let act = |a: &PlayableAction| {
                matches!(a, PlayableAction::Activate { .. } | PlayableAction::ActivateMana { .. })
            };
            // "Productive" = any non-pass game action (cast / land / activate). `cast_rate` and
            // `playland_rate` are per-window action *selection* rates, so they cap below 1.0 for an
            // optimal policy: when a cast and a land drop are BOTH legal in one window, taking one
            // scores a miss against the other. `productive_rate = productive_taken/productive_legal`
            // has no such artifact — it asks "when something useful was possible, did you do
            // something useful (vs pass)?" and → 1.0 for optimal play. OR-combined per window here
            // because it can't be recovered from the summed cast/land/activate fields downstream.
            // Non-capturing (inlined, not `cast||land||act`) so it still coerces to the `fn` pointer
            // `any`/`is` take — a closure that captured the others could not.
            let productive = |a: &PlayableAction| {
                matches!(
                    a,
                    PlayableAction::Cast { .. }
                        | PlayableAction::PlayLand { .. }
                        | PlayableAction::Activate { .. }
                        | PlayableAction::ActivateMana { .. }
                        // A prepared-spell cast (SoS Prepare) is a real game action, not a pass.
                        | PlayableAction::CastPrepared { .. }
                )
            };
            vec![
                ("priority_windows", 1.0),
                ("cast_legal", b(any(cast))),
                ("cast_taken", b(is(chosen, cast))),
                ("playland_legal", b(any(land))),
                ("playland_taken", b(is(chosen, land))),
                ("activate_legal", b(any(act))),
                ("activate_taken", b(is(chosen, act))),
                ("productive_legal", b(any(productive))),
                ("productive_taken", b(is(chosen, productive))),
                ("priority_passed", b(matches!(resp, DecisionResponse::Pass))),
            ]
        }
        // Combat: eligible creatures vs creatures actually declared (each pair is one declarer).
        Q::DeclareAttackers { eligible } => vec![
            ("attack_eligible", eligible.len() as f64),
            ("attack_declared", declared(resp)),
        ],
        Q::DeclareBlockers { eligible, .. } => {
            // Also the *shape* of the blocks: how many distinct attackers were blocked, and how many
            // were DOUBLE-blocked (≥2 blockers ganging one attacker). Double-blocking is the
            // sophisticated anti-trample play — two 2/2s kill a 3/3 Swine instead of one chumping into
            // it — so `block_double` distinguishes "gang the trampler" from "single-block everything"
            // even when `block_rate` is 1.0. Read off the response `Pairs` only (no game state).
            let (attackers_blocked, block_double) = block_shape(resp);
            vec![
                ("block_eligible", eligible.len() as f64),
                ("block_declared", declared(resp)),
                ("attackers_blocked", attackers_blocked),
                ("block_double", block_double),
            ]
        }
        _ => vec![],
    }
}

/// From a block response `Pairs((blocker, attacker))`: (# distinct attackers blocked, # attackers
/// assigned ≥2 blockers). Data-only — groups by the attacker element, no characteristics needed.
fn block_shape(resp: &DecisionResponse) -> (f64, f64) {
    let mut per_attacker: std::collections::BTreeMap<u32, u32> = std::collections::BTreeMap::new();
    if let DecisionResponse::Pairs(p) = resp {
        for (_blocker, attacker) in p {
            *per_attacker.entry(*attacker).or_insert(0) += 1;
        }
    }
    let blocked = per_attacker.len() as f64;
    let doubles = per_attacker.values().filter(|&&c| c >= 2).count() as f64;
    (blocked, doubles)
}

/// How many declarers a combat response committed (the codec commits `Pairs`, one per declarer).
fn declared(resp: &DecisionResponse) -> f64 {
    match resp {
        DecisionResponse::Pairs(p) => p.len() as f64,
        DecisionResponse::Indices(i) => i.len() as f64,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::agent::{AttackerOption, BlockerOption, CastVariant};
    use mtg_core::ids::ObjId;

    fn field(rec: &[(&'static str, f64)], key: &str) -> f64 {
        rec.iter().find(|(k, _)| *k == key).map(|(_, v)| *v).unwrap_or(0.0)
    }

    #[test]
    fn priority_cast_legal_and_taken() {
        let actions = vec![
            PlayableAction::Cast { spell: ObjId(1), variant: CastVariant::Normal },
            PlayableAction::PlayLand { card: ObjId(2) },
        ];
        let req = DecisionRequest::Priority { actions, can_pass: true };
        // Took the cast (action 0). Both cast and land were legal, so playland scores a miss even
        // though a productive action WAS taken — hence productive_taken=1 (the artifact-free view).
        let rec = summarize(&req, &DecisionResponse::Action(0));
        assert_eq!(field(&rec, "cast_legal"), 1.0);
        assert_eq!(field(&rec, "cast_taken"), 1.0);
        assert_eq!(field(&rec, "playland_legal"), 1.0);
        assert_eq!(field(&rec, "playland_taken"), 0.0);
        assert_eq!(field(&rec, "priority_windows"), 1.0);
        assert_eq!(field(&rec, "productive_legal"), 1.0);
        assert_eq!(field(&rec, "productive_taken"), 1.0);
        // Passed instead: cast was legal but not taken, and nothing productive was taken.
        let rec = summarize(&req, &DecisionResponse::Pass);
        assert_eq!(field(&rec, "cast_legal"), 1.0);
        assert_eq!(field(&rec, "cast_taken"), 0.0);
        assert_eq!(field(&rec, "priority_passed"), 1.0);
        assert_eq!(field(&rec, "productive_legal"), 1.0);
        assert_eq!(field(&rec, "productive_taken"), 0.0);
    }

    #[test]
    fn attackers_eligible_vs_declared() {
        let opt = |id| AttackerOption {
            creature: ObjId(id),
            may_attack: vec![],
            required: false,
            attack_cost: None,
            may_exert: false,
            may_enlist: false,
        };
        let req = DecisionRequest::DeclareAttackers { eligible: vec![opt(1), opt(2), opt(3)] };
        // Declared two of three eligible (two attacker→defender pairs).
        let rec = summarize(&req, &DecisionResponse::Pairs(vec![(0, 0), (1, 0)]));
        assert_eq!(field(&rec, "attack_eligible"), 3.0);
        assert_eq!(field(&rec, "attack_declared"), 2.0);
    }

    #[test]
    fn blockers_shape_counts_double_blocks() {
        let blk = |id| BlockerOption { creature: ObjId(id), may_block: vec![], required: false, block_cost: None };
        let req = DecisionRequest::DeclareBlockers {
            eligible: vec![blk(1), blk(2), blk(3)],
            attackers: vec![ObjId(10), ObjId(11)],
        };
        // Pairs (blocker, attacker): blockers 0 & 1 gang attacker 0 (double-block), blocker 2 blocks attacker 1.
        let rec = summarize(&req, &DecisionResponse::Pairs(vec![(0, 0), (1, 0), (2, 1)]));
        assert_eq!(field(&rec, "block_eligible"), 3.0);
        assert_eq!(field(&rec, "block_declared"), 3.0);
        assert_eq!(field(&rec, "attackers_blocked"), 2.0); // attackers 0 and 1
        assert_eq!(field(&rec, "block_double"), 1.0);      // only attacker 0 is ganged
    }
}
