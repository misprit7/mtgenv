//! A **temporary** minimal lands-only game driver, used as scaffolding until the engine's real
//! turn/priority loop (board task #7) lands a `run_game(state, agents)` entry point.
//!
//! It exercises the full [`Agent`] boundary end-to-end — choose-starting-player, mulligan,
//! priority (play-a-land / pass), draw, deck-out loss, discard-to-hand-size — so the CLI (M1)
//! and the web client (M2) are demonstrably playing a real game *through the boundary*, not a
//! mock. It deliberately implements only as much rules logic as a lands-only game needs, using
//! `mtg-core`'s **public** API (no engine internals). When #7 lands, this file is replaced by a
//! call into the engine's loop; the agents, the views, and the option projection are unchanged.

use mtg_core::agent::{
    Agent, DecisionRequest, DecisionResponse, GameEvent, PlayableAction, SelectReason,
};
use mtg_core::basics::{Phase, Zone};
use mtg_core::ids::{ObjId, PlayerId};
use mtg_core::state::view::view_for;
use mtg_core::state::{Characteristics, GameState, Player};

/// How a game ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub winner: Option<PlayerId>,
    pub turns: u32,
}

/// The five basic land names, dealt round-robin into each library.
const BASICS: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];
/// Library size before the opening draw (small so a lands-only game ends by deck-out quickly).
const LIBRARY_SIZE: usize = 14;
/// A turn-count backstop so a misbehaving agent can never loop forever.
const MAX_TURNS: u32 = 400;

/// Run one lands-only game between `agents` (indexed by seat). Returns the [`Outcome`].
pub fn run_lands_game(mut agents: Vec<Box<dyn Agent>>, seed: u64) -> Outcome {
    let mut state = setup(seed, agents.len());

    // Initial push so clients render the opening board before the first prompt.
    broadcast(
        &state,
        &mut agents,
        &GameEvent::PhaseBegan {
            turn: state.turn_number,
            phase: state.phase,
            active: state.active_player,
        },
    );

    // Choose the starting player (asked of seat 0).
    let candidates = state.living_players();
    let req = DecisionRequest::ChooseStartingPlayer {
        candidates: candidates.clone(),
    };
    let view = view_for(&state, PlayerId(0));
    let start = match agents[0].decide(&view, &req) {
        DecisionResponse::Index(i) => candidates.get(i as usize).copied().unwrap_or(PlayerId(0)),
        _ => PlayerId(0),
    };
    state.starting_player = start;
    state.active_player = start;

    // London-style mulligan, one resolved decision per seat (simplified: no interactive bottom).
    for seat in 0..agents.len() {
        mulligan_phase(&mut state, &mut agents, PlayerId(seat as u32));
    }

    // Turn loop.
    while !state.game_over && state.turn_number <= MAX_TURNS {
        take_turn(&mut state, &mut agents);
        check_game_over(&mut state, &mut agents);
    }

    Outcome {
        winner: state.winner,
        turns: state.turn_number,
    }
}

// ── Setup ────────────────────────────────────────────────────────────────────────────────────

fn setup(seed: u64, num_players: usize) -> GameState {
    let mut state = GameState::new(num_players, seed);
    for seat in 0..num_players as u32 {
        let pid = PlayerId(seat);
        for i in 0..LIBRARY_SIZE {
            let name = BASICS[i % BASICS.len()];
            state.add_card(pid, Characteristics::basic_land(name), Zone::Library);
        }
        state.shuffle_library(pid);
        draw(&mut state, pid, 7);
    }
    state.starting_player = PlayerId(0);
    state.active_player = PlayerId(0);
    state.turn_number = 1;
    state
}

// ── Per-turn structure ─────────────────────────────────────────────────────────────────────

fn take_turn(state: &mut GameState, agents: &mut Vec<Box<dyn Agent>>) {
    let ap = state.active_player;

    // Untap step: untap this player's permanents, clear summoning sickness.
    state.phase = Phase::Untap;
    let bf = state.player(ap).battlefield.clone();
    for id in bf {
        if let Some(o) = state.objects.get_mut(&id) {
            o.status.tapped = false;
            o.summoning_sick = false;
        }
    }
    announce_phase(state, agents);

    // Upkeep (no triggers in a lands-only game).
    state.phase = Phase::Upkeep;
    announce_phase(state, agents);

    // Draw step (the starting player skips their first draw, CR 103.8a).
    state.phase = Phase::Draw;
    announce_phase(state, agents);
    let first_turn_of_starter = state.turn_number == 1 && ap == state.starting_player;
    if !first_turn_of_starter {
        draw_one(state, agents, ap);
    }
    if state.player(ap).has_lost {
        return;
    }

    // Precombat main: priority to play a land / pass.
    state.phase = Phase::PrecombatMain;
    state.player_mut(ap).lands_played_this_turn = 0;
    announce_phase(state, agents);
    main_phase_priority(state, agents, ap);

    // (Combat / postcombat are skipped — lands-only.)

    // End step + cleanup (discard to hand size, empty mana pools).
    state.phase = Phase::End;
    announce_phase(state, agents);
    state.phase = Phase::Cleanup;
    cleanup_discard(state, agents, ap);
    for p in state.players.iter_mut() {
        p.mana_pool = Default::default();
    }

    advance_turn(state);
}

/// Give the active player priority in their main phase: repeatedly offer the legal land plays
/// plus pass until they pass.
fn main_phase_priority(state: &mut GameState, agents: &mut Vec<Box<dyn Agent>>, ap: PlayerId) {
    loop {
        let actions = land_plays(state, ap);
        let req = DecisionRequest::Priority {
            actions: actions.clone(),
            can_pass: true,
        };
        let view = view_for(state, ap);
        match agents[ap.0 as usize].decide(&view, &req) {
            DecisionResponse::Pass => break,
            DecisionResponse::Action(i) => {
                if let Some(action) = actions.get(i as usize).cloned() {
                    apply_action(state, agents, ap, &action);
                } else {
                    break;
                }
            }
            _ => break,
        }
    }
}

/// The legal land plays for `ap` right now (CR 116.2a: one land per turn by default).
fn land_plays(state: &GameState, ap: PlayerId) -> Vec<PlayableAction> {
    if state.player(ap).lands_played_this_turn >= 1 {
        return vec![];
    }
    state
        .player(ap)
        .hand
        .iter()
        .filter(|&&id| state.object(id).chars.is_land())
        .map(|&id| PlayableAction::PlayLand { card: id })
        .collect()
}

fn apply_action(
    state: &mut GameState,
    agents: &mut Vec<Box<dyn Agent>>,
    ap: PlayerId,
    action: &PlayableAction,
) {
    if let PlayableAction::PlayLand { card } = action {
        move_card(state, *card, Zone::Battlefield, ap);
        state.player_mut(ap).lands_played_this_turn += 1;
        broadcast(
            state,
            agents,
            &GameEvent::ObjectMoved {
                obj: *card,
                to: Zone::Battlefield,
            },
        );
    }
}

/// Discard down to hand size at cleanup (CR 514.1) — exercises `SelectCards`.
fn cleanup_discard(state: &mut GameState, agents: &mut Vec<Box<dyn Agent>>, ap: PlayerId) {
    let hand = state.player(ap).hand.clone();
    let limit = state.player(ap).hand_size_limit;
    if hand.len() <= limit {
        return;
    }
    let excess = (hand.len() - limit) as u32;
    let req = DecisionRequest::SelectCards {
        reason: SelectReason::DiscardToHandSize,
        from: hand.clone(),
        min: excess,
        max: excess,
        description: format!("Discard {excess} card(s) to hand size"),
    };
    let view = view_for(state, ap);
    let picks = match agents[ap.0 as usize].decide(&view, &req) {
        DecisionResponse::Indices(v) => v,
        _ => vec![],
    };
    let mut chosen: Vec<ObjId> = picks
        .iter()
        .filter_map(|&i| hand.get(i as usize).copied())
        .collect();
    chosen.truncate(excess as usize);
    // If the agent under-selected, fill from the front so the rule still completes.
    for &c in &hand {
        if chosen.len() >= excess as usize {
            break;
        }
        if !chosen.contains(&c) {
            chosen.push(c);
        }
    }
    for c in chosen {
        move_card(state, c, Zone::Graveyard, ap);
        broadcast(
            state,
            agents,
            &GameEvent::ObjectMoved {
                obj: c,
                to: Zone::Graveyard,
            },
        );
    }
}

/// One London mulligan decision for `seat` (simplified: a `Mulligan` reshuffles and redraws 7;
/// kept hands bottom `mulligans_taken` cards non-interactively; capped at 3).
fn mulligan_phase(state: &mut GameState, agents: &mut Vec<Box<dyn Agent>>, seat: PlayerId) {
    let mut taken: u32 = 0;
    loop {
        let hand = state.player(seat).hand.clone();
        let req = DecisionRequest::Mulligan {
            hand,
            mulligans_taken: taken,
            will_bottom_if_kept: taken,
        };
        let view = view_for(state, seat);
        let mull = matches!(
            agents[seat.0 as usize].decide(&view, &req),
            DecisionResponse::Bool(true)
        );
        if !mull || taken >= 3 {
            // Keep: bottom `taken` cards (non-interactive; all basics, so any choice is equal).
            for _ in 0..taken {
                if let Some(&c) = state.player(seat).hand.first() {
                    move_card(state, c, Zone::Library, seat);
                }
            }
            break;
        }
        // Mulligan: shuffle the whole hand back and redraw seven.
        let hand = state.player(seat).hand.clone();
        for c in hand {
            move_card(state, c, Zone::Library, seat);
        }
        state.shuffle_library(seat);
        draw(state, seat, 7);
        taken += 1;
    }
}

// ── State-change helpers (public-API only; mirrors engine move/draw until #7 lands) ─────────

fn advance_turn(state: &mut GameState) {
    let n = state.players.len() as u32;
    state.active_player = PlayerId((state.active_player.0 + 1) % n);
    state.turn_number += 1;
}

fn check_game_over(state: &mut GameState, agents: &mut Vec<Box<dyn Agent>>) {
    let living = state.living_players();
    if living.len() <= 1 {
        state.game_over = true;
        state.winner = living.first().copied();
        broadcast(
            state,
            agents,
            &GameEvent::GameEnded {
                winner: state.winner,
            },
        );
    }
}

/// Draw `n` cards for `seat` at setup (no events; opening hands / mulligans).
fn draw(state: &mut GameState, seat: PlayerId, n: usize) {
    for _ in 0..n {
        let c = state.player_mut(seat).library.pop();
        match c {
            Some(c) => {
                state.player_mut(seat).hand.push(c);
                if let Some(o) = state.objects.get_mut(&c) {
                    o.zone = Zone::Hand;
                }
            }
            None => {
                state.player_mut(seat).drew_from_empty = true;
                state.player_mut(seat).has_lost = true;
            }
        }
    }
}

/// Draw one card during the draw step; deck-out (CR 104.3a/704.5c) marks the loss.
fn draw_one(state: &mut GameState, agents: &mut Vec<Box<dyn Agent>>, seat: PlayerId) {
    let c = state.player_mut(seat).library.pop();
    match c {
        Some(c) => {
            state.player_mut(seat).hand.push(c);
            if let Some(o) = state.objects.get_mut(&c) {
                o.zone = Zone::Hand;
            }
            broadcast(
                state,
                agents,
                &GameEvent::DrewCards {
                    player: seat,
                    count: 1,
                },
            );
        }
        None => {
            state.player_mut(seat).drew_from_empty = true;
            state.player_mut(seat).has_lost = true;
        }
    }
}

/// Move a card between per-player zones, keeping the object's `zone`/`controller` and the zone
/// vectors in sync. (Local stand-in for the engine's `pub(crate)` `move_object`.)
fn move_card(state: &mut GameState, card: ObjId, to: Zone, to_owner: PlayerId) {
    if let Some(o) = state.objects.get(&card) {
        let (from, owner) = (o.zone, o.owner);
        if let Some(v) = zone_vec_mut(state.player_mut(owner), from) {
            v.retain(|&x| x != card);
        }
    }
    if let Some(o) = state.objects.get_mut(&card) {
        o.zone = to;
        o.controller = if to == Zone::Battlefield { to_owner } else { o.owner };
        if to != Zone::Battlefield {
            o.status = Default::default();
        }
    }
    if let Some(v) = zone_vec_mut(state.player_mut(to_owner), to) {
        v.push(card);
    }
}

fn zone_vec_mut(p: &mut Player, z: Zone) -> Option<&mut Vec<ObjId>> {
    match z {
        Zone::Library => Some(&mut p.library),
        Zone::Hand => Some(&mut p.hand),
        Zone::Battlefield => Some(&mut p.battlefield),
        Zone::Graveyard => Some(&mut p.graveyard),
        Zone::Exile => Some(&mut p.exile),
        Zone::Stack | Zone::Command => None,
    }
}

// ── Event fan-out (the observe() push channel) ───────────────────────────────────────────────

fn announce_phase(state: &GameState, agents: &mut Vec<Box<dyn Agent>>) {
    let ev = GameEvent::PhaseBegan {
        turn: state.turn_number,
        phase: state.phase,
        active: state.active_player,
    };
    broadcast(state, agents, &ev);
}

/// Push a public event to every seat with its own information-filtered view.
fn broadcast(state: &GameState, agents: &mut Vec<Box<dyn Agent>>, ev: &GameEvent) {
    for seat in 0..agents.len() {
        let view = view_for(state, PlayerId(seat as u32));
        agents[seat].observe(&view, ev);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::agent::RandomAgent;

    #[test]
    fn random_vs_random_terminates_with_a_winner() {
        // The boundary guarantees only-legal options, so two RandomAgents always finish a
        // lands-only game (by deck-out) within the turn backstop, deterministically per seed.
        let agents: Vec<Box<dyn Agent>> =
            vec![Box::new(RandomAgent::new(1)), Box::new(RandomAgent::new(2))];
        let outcome = run_lands_game(agents, 42);
        assert!(outcome.winner.is_some(), "game should produce a winner");
        assert!(outcome.turns <= MAX_TURNS, "game must terminate");
    }

    #[test]
    fn outcome_is_deterministic_for_seed() {
        let make = || -> Vec<Box<dyn Agent>> {
            vec![Box::new(RandomAgent::new(7)), Box::new(RandomAgent::new(9))]
        };
        let a = run_lands_game(make(), 123);
        let b = run_lands_game(make(), 123);
        assert_eq!(a, b);
    }
}
