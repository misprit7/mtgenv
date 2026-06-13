//! The milestone-0 **observation encoder** — the swappable seam from the engine's info-filtered
//! [`PlayerView`] to a fixed-width `f32` vector the Python policy consumes.
//!
//! It reads the same [`PlayerView`] the `Agent` boundary already produces, so hidden-information
//! masking is inherited, not re-done: the encoder never sees `GameState`, so a leak is
//! structurally impossible (GYM_PLAN §3). Milestone 0 encodes only **global scalars** + a
//! decision-context one-hot — enough to drive random self-play and exercise the plumbing. The
//! richer per-permanent / per-card-in-hand / stack rows with `grp_id` embedding ids (GYM_PLAN §3)
//! are milestone 1; they slot in here without touching the `PyGame` plumbing or the env.

use mtg_core::agent::{DecisionRequest, PlayerView};
use mtg_core::basics::Phase;

/// The 12 `Phase` values, in enum order — the one-hot basis for the phase feature.
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

/// Number of `DecisionRequest` variants (the decision-context one-hot width). Keep in sync with
/// [`request_index`].
const NUM_REQUESTS: usize = 21;

/// Per-seat scalar block width (life, poison, hand, library, graveyard, exile, mana).
const SEAT_BLOCK: usize = 7;

/// The fixed observation width. Layout (all `f32`):
/// `[turn] [phase one-hot ×12] [active_is_me] [prio_is_me] [prio_exists]`
/// `[request one-hot ×21] [num_legal_norm] [stack_depth] [battlefield_total]`
/// `[me: ×7] [opp: ×7]`.
pub const OBS_DIM: usize = 1 + 12 + 3 + NUM_REQUESTS + 3 + SEAT_BLOCK + SEAT_BLOCK; // = 54

/// Encode `view` + the current `req` (and its legal-option count) into a fixed `OBS_DIM` vector.
/// All entries are finite; variable-length structure is summarized to scalars for milestone 0.
pub fn encode(view: &PlayerView, req: &DecisionRequest, num_legal: usize) -> Vec<f32> {
    let mut o = Vec::with_capacity(OBS_DIM);

    o.push(view.turn as f32);

    for ph in PHASES {
        o.push((view.phase == ph) as u8 as f32);
    }

    let me = view.seat;
    o.push((view.active_player == me) as u8 as f32);
    o.push((view.priority_player == Some(me)) as u8 as f32);
    o.push(view.priority_player.is_some() as u8 as f32);

    let ridx = request_index(req);
    for i in 0..NUM_REQUESTS {
        o.push((i == ridx) as u8 as f32);
    }

    o.push(num_legal as f32);
    o.push(view.stack.len() as f32);
    o.push(view.battlefield.len() as f32);

    // Self block, then the first opponent's block (zeros if somehow absent).
    let my_pub = view.players.iter().find(|p| p.player == me);
    let opp_pub = view.players.iter().find(|p| p.player != me);
    push_seat(&mut o, my_pub.map(|p| (p, view.me.hand.len() as u32)));
    push_seat(&mut o, opp_pub.map(|p| (p, p.hand_count)));

    debug_assert_eq!(o.len(), OBS_DIM);
    o
}

/// Append one seat's 7 scalar features. `hand` is passed explicitly because the seat's *own* hand
/// is fully visible (`view.me.hand`) while an opponent's is only a count (`hand_count`).
fn push_seat(o: &mut Vec<f32>, seat: Option<(&mtg_core::agent::PlayerPublicView, u32)>) {
    match seat {
        Some((p, hand)) => {
            o.push(p.life as f32);
            o.push(p.poison as f32);
            o.push(hand as f32);
            o.push(p.library_count as f32);
            o.push(p.graveyard.len() as f32);
            o.push(p.exile_public.len() as f32);
            o.push(p.mana_pool.total() as f32);
        }
        None => o.extend(std::iter::repeat(0.0).take(SEAT_BLOCK)),
    }
}

/// Stable index of each [`DecisionRequest`] variant for the decision-context one-hot (GYM_PLAN
/// §3). Order is fixed; changing it changes the observation, so append new variants at the end.
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
            hand_count: 3,
            library_count: 20,
            graveyard: vec![],
            exile_public: vec![],
            mana_pool: ManaPool::default(),
            counters: CounterBag::default(),
        }
    }

    fn view() -> PlayerView {
        PlayerView {
            seat: PlayerId(0),
            turn: 4,
            active_player: PlayerId(0),
            phase: Phase::PrecombatMain,
            priority_player: Some(PlayerId(0)),
            players: vec![pub_view(0, 20), pub_view(1, 17)],
            me: PlayerPrivateView { hand: vec![], known_library: vec![], revealed_to_me: vec![] },
            battlefield: vec![],
            stack: vec![],
            combat: None,
            stops: None,
        }
    }

    #[test]
    fn encode_is_fixed_width_and_finite() {
        let req = DecisionRequest::Priority { actions: vec![], can_pass: true };
        let o = encode(&view(), &req, 1);
        assert_eq!(o.len(), OBS_DIM);
        assert!(o.iter().all(|x| x.is_finite()));
        // turn is first; PrecombatMain one-hot is set; self life (20) precedes opp life (17).
        assert_eq!(o[0], 4.0);
        assert_eq!(o[1 + 3], 1.0, "PrecombatMain one-hot");
    }

    #[test]
    fn request_index_is_unique_over_21_variants() {
        // A quick guard that the one-hot basis stays a bijection onto 0..21.
        let idxs: std::collections::BTreeSet<usize> = (0..NUM_REQUESTS).collect();
        assert_eq!(idxs.len(), NUM_REQUESTS);
    }
}
