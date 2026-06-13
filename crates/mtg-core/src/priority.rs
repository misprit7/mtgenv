//! The priority loop + the agenda pipeline, plus the turn driver that ties the engine
//! together (the [`Engine`]). Run to a fixpoint between priority passes:
//! recompute continuous effects → state-based actions (loop) → put triggers on the
//! stack (APNAP) → grant priority. CR 117.5, 603.3, 704.3.
//!
//! See `docs/design/WHITEBOARD_MODEL.md` §2.2 — the agenda order is *law* and is covered
//! by tests at the bottom of this file.
//!
//! Milestone 2 scope: a lands-only game. The only priority action is "play a land"
//! (CR 116.2a, a special action). Casting/mana/combat declarations arrive in milestone 3;
//! the loop is written generically (it resolves the stack, drains triggers, etc.) so those
//! slot in without reshaping it.

use crate::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayableAction, SelectReason};
use crate::basics::{Phase, Zone};
use crate::ids::PlayerId;
use crate::sba::{self, LossReason, StateBasedAction};
use crate::stack::StackObjectKind;
use crate::state::view::view_for;
use crate::state::GameState;
use crate::turn::{is_main_phase, step_grants_priority, TURN_STEPS};

/// A hard cap on turns so a pathological game can never loop forever. Real games end far
/// sooner (a lands-only game ends when a player decks out, CR 704.5b). Reaching the cap
/// ends the game as a draw.
const MAX_TURNS: u32 = 2000;

/// The engine: full [`GameState`] plus one [`Agent`] per seat (indexed by `PlayerId.0`).
/// All player choices flow through the agents; nothing else asks a player anything.
pub struct Engine {
    pub state: GameState,
    agents: Vec<Box<dyn Agent>>,
    /// Append-only record of every public event broadcast this game (the same stream sent
    /// to agents' `observe`). Handy for a CLI trace and for snapshot tests. Off by default.
    pub event_log: Vec<GameEvent>,
    record_events: bool,
    started: bool,
}

impl Engine {
    /// `agents` must have one entry per seat in `state`, in `PlayerId` order.
    pub fn new(state: GameState, agents: Vec<Box<dyn Agent>>) -> Self {
        assert_eq!(
            agents.len(),
            state.players.len(),
            "one agent per seat is required"
        );
        Engine {
            state,
            agents,
            event_log: Vec::new(),
            record_events: false,
            started: false,
        }
    }

    /// Enable recording of broadcast events into [`Engine::event_log`] (for tracing/tests).
    pub fn record_events(&mut self, on: bool) {
        self.record_events = on;
    }

    // ── top-level driver ────────────────────────────────────────────────────────────────

    /// Deal opening hands (once) and play turns until the game ends. Returns the winner
    /// (`None` = draw / no surviving player).
    pub fn run_game(&mut self) -> Option<PlayerId> {
        self.start_game();
        while !self.state.game_over && self.state.turn_number <= MAX_TURNS {
            self.take_turn();
        }
        if !self.state.game_over {
            // Hit the safety cap: end as a draw.
            self.state.game_over = true;
        }
        self.state.winner
    }

    /// Pre-game setup: shuffle libraries and draw opening hands (CR 103.5). Mulligans are
    /// deferred (milestone 2 keeps every opening hand). Idempotent.
    pub fn start_game(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        let hand_size = crate::state::DEFAULT_HAND_SIZE as u32;
        let seats: Vec<PlayerId> = self.state.players.iter().map(|p| p.id).collect();
        for &p in &seats {
            self.state.shuffle_library(p);
        }
        // Opening draws are not "draw step" draws and don't risk decking on a normal deck.
        for &p in &seats {
            self.draw(p, hand_size);
        }
    }

    /// Run one whole turn for the current active player.
    fn take_turn(&mut self) {
        self.begin_turn();
        for &step in TURN_STEPS.iter() {
            if self.state.game_over {
                return;
            }
            self.run_step(step);
        }
        if !self.state.game_over {
            self.advance_turn();
        }
    }

    /// Start-of-turn housekeeping (before the untap step's own actions): reset the
    /// land-drop and clear summoning sickness for permanents the active player has
    /// controlled since the turn began (CR 302.6).
    fn begin_turn(&mut self) {
        let ap = self.state.active_player;
        self.state.player_mut(ap).lands_played_this_turn = 0;
        let perms = self.state.player(ap).battlefield.clone();
        for id in perms {
            if let Some(o) = self.state.objects.get_mut(&id) {
                o.summoning_sick = false;
            }
        }
    }

    /// Move to the next player's turn (CR 500.1 wrap-around). In two-player this just
    /// alternates seats.
    fn advance_turn(&mut self) {
        self.empty_mana_pools();
        let n = self.state.players.len();
        let cur = self.state.active_player.0 as usize;
        let next = (cur + 1) % n;
        self.state.active_player = self.state.players[next].id;
        self.state.turn_number += 1;
        self.state.phase = Phase::Untap;
    }

    // ── steps & turn-based actions ────────────────────────────────────────────────────────

    fn run_step(&mut self, step: Phase) {
        self.state.phase = step;
        let ev = GameEvent::PhaseBegan {
            turn: self.state.turn_number,
            phase: step,
            active: self.state.active_player,
        };
        self.broadcast(ev);

        // Turn-based actions happen first, before any priority (CR 703.3 / 117.3a).
        self.turn_based_actions(step);

        if step == Phase::Cleanup {
            self.cleanup_step();
        } else if step_grants_priority(step) {
            self.priority_round();
        }
        // Mana pools empty as each step/phase ends (CR 500.5 / 514.3-era timing).
        self.empty_mana_pools();
    }

    /// The turn-based actions for a step (CR 703 / the RULES_SUMMARY §3 table).
    fn turn_based_actions(&mut self, step: Phase) {
        match step {
            Phase::Untap => {
                // (1) phasing and (2) day/night are no-ops for milestone 2. (3) Untap all
                // of the active player's permanents (CR 502.3).
                let ap = self.state.active_player;
                let perms = self.state.player(ap).battlefield.clone();
                for id in perms {
                    if let Some(o) = self.state.objects.get_mut(&id) {
                        o.status.tapped = false;
                    }
                }
            }
            Phase::Draw => {
                // The active player draws a card (CR 504.1), unless this is the first turn
                // and they are the starting player in a two-player game (CR 103.8a).
                let ap = self.state.active_player;
                let skip = self.state.turn_number == 1 && ap == self.state.starting_player;
                if !skip {
                    self.draw(ap, 1);
                }
            }
            // Combat declarations (508/509) and all other steps have no milestone-2
            // turn-based actions: a lands-only game has no creatures to declare. (Combat
            // lands in milestone 3 — see combat/.)
            _ => {}
        }
    }

    /// Cleanup (CR 514): discard to maximum hand size, then remove marked damage and end
    /// "until end of turn" effects, simultaneously. Normally no priority (514.3); the
    /// 514.3a exception (pending SBAs/triggers ⇒ grant priority, then repeat) is handled
    /// by running the agenda and, only if it left something on the stack, a priority round.
    fn cleanup_step(&mut self) {
        let ap = self.state.active_player;
        // (1) Discard to maximum hand size (CR 514.1).
        self.discard_to_hand_size(ap);
        // (2) Remove all marked damage; end "until end of turn"/"this turn" effects (514.2).
        // (No such effects exist yet in milestone 2.)
        for o in self.state.objects.values_mut() {
            if o.zone == Zone::Battlefield {
                o.damage_marked = 0;
            }
        }
        // CR 514.3 / 514.3a: check SBAs and triggers. If that puts something on the stack,
        // the active player gets priority and we repeat cleanup; otherwise the step ends.
        self.run_agenda();
        if !self.state.game_over && !self.state.stack.is_empty() {
            self.priority_round();
            if !self.state.game_over {
                self.cleanup_step();
            }
        }
    }

    fn discard_to_hand_size(&mut self, p: PlayerId) {
        let limit = self.state.player(p).hand_size_limit;
        let hand = self.state.player(p).hand.clone();
        if hand.len() <= limit {
            return;
        }
        let excess = (hand.len() - limit) as u32;
        let req = DecisionRequest::SelectCards {
            reason: SelectReason::DiscardToHandSize,
            from: hand.clone(),
            min: excess,
            max: excess,
            description: format!("discard down to {limit} cards"),
        };
        let chosen = match self.ask(p, &req) {
            DecisionResponse::Indices(idxs) => self.distinct_valid_indices(&idxs, hand.len(), excess),
            _ => (0..excess as usize).collect(),
        };
        // Discard highest index first so earlier indices stay valid.
        let mut to_discard: Vec<usize> = chosen;
        to_discard.sort_unstable();
        to_discard.dedup();
        for &i in to_discard.iter().rev() {
            let card = hand[i];
            let owner = self.state.object(card).owner;
            self.state.move_object(card, Zone::Graveyard, owner);
            self.broadcast(GameEvent::ObjectMoved {
                obj: card,
                to: Zone::Graveyard,
            });
        }
    }

    /// Pick exactly `want` distinct in-range indices from an agent's (possibly malformed)
    /// response — defensive so a buggy agent can never panic the engine (the contract says
    /// it won't, but we don't trust it). Falls back to the lowest indices.
    fn distinct_valid_indices(&self, idxs: &[u32], n: usize, want: u32) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        for &i in idxs {
            let i = i as usize;
            if i < n && !out.contains(&i) {
                out.push(i);
            }
            if out.len() == want as usize {
                break;
            }
        }
        let mut fill = 0;
        while out.len() < want as usize && fill < n {
            if !out.contains(&fill) {
                out.push(fill);
            }
            fill += 1;
        }
        out
    }

    // ── the priority round ────────────────────────────────────────────────────────────────

    /// One step's priority round (CR 117). The active player gets priority first; players
    /// pass in turn order. When all pass in succession, the top of the stack resolves (or,
    /// if empty, the step/phase ends — CR 117.4, 500.2). Before *any* player receives
    /// priority, the agenda fixpoint runs (CR 117.5).
    fn priority_round(&mut self) {
        let order = self.turn_order();
        let n = order.len();
        let mut idx = 0usize; // whose priority: index into `order` (starts at active player)
        let mut passes = 0usize; // consecutive passes

        loop {
            self.run_agenda();
            if self.state.game_over {
                self.state.priority_player = None;
                return;
            }
            let p = order[idx];
            self.state.priority_player = Some(p);

            let actions = self.legal_priority_actions(p);
            let req = DecisionRequest::Priority {
                actions: actions.clone(),
                can_pass: true,
            };
            match self.ask(p, &req) {
                DecisionResponse::Action(i) if (i as usize) < actions.len() => {
                    self.perform_priority_action(p, &actions[i as usize]);
                    // The player who acted retains priority (CR 117.3c / 116.3): `idx`
                    // stays put, and any prior passes are voided.
                    passes = 0;
                }
                // Pass (explicit, or any out-of-range/ill-typed response treated as a pass
                // so a misbehaving agent can never wedge the loop).
                _ => {
                    passes += 1;
                    if passes >= n {
                        if self.state.stack.is_empty() {
                            // All passed with an empty stack: the step/phase ends (CR 500.2).
                            self.state.priority_player = None;
                            return;
                        }
                        // All passed: resolve the top of the stack (CR 117.4), then the
                        // active player gets priority again (CR 117.3b).
                        self.resolve_top();
                        passes = 0;
                        idx = 0;
                    } else {
                        idx = (idx + 1) % n;
                    }
                }
            }
        }
    }

    /// Turn order for priority purposes (CR 101.4): active player first, then the others in
    /// turn order. Two-player: `[active, other]`.
    fn turn_order(&self) -> Vec<PlayerId> {
        let n = self.state.players.len();
        let ap = self.state.active_player.0 as usize;
        (0..n)
            .map(|k| self.state.players[(ap + k) % n].id)
            .collect()
    }

    /// Enumerate the legal actions `p` may take with priority right now (the engine's job:
    /// masking, CR 117). Milestone 2: only playing a land (CR 116.2a / 505.6).
    fn legal_priority_actions(&self, p: PlayerId) -> Vec<PlayableAction> {
        let mut actions = Vec::new();
        let s = &self.state;
        let can_play_land = p == s.active_player
            && is_main_phase(s.phase)
            && s.stack.is_empty()
            && s.player(p).lands_played_this_turn < 1;
        if can_play_land {
            for &card in &s.player(p).hand {
                if s.object(card).chars.is_land() {
                    actions.push(PlayableAction::PlayLand { card });
                }
            }
        }
        actions
    }

    // The action-dispatch point; gains Cast/Activate/Special arms in milestone 3, so it is
    // kept as a `match` even though only one arm exists today.
    #[allow(clippy::single_match)]
    fn perform_priority_action(&mut self, p: PlayerId, action: &PlayableAction) {
        match action {
            PlayableAction::PlayLand { card } => self.play_land(p, *card),
            // Cast/Activate/Special: milestone 3+. Ignored defensively for now (they are
            // never enumerated as legal in milestone 2, so this is unreachable in practice).
            _ => {}
        }
    }

    /// Play a land: a special action (CR 116.2a), no stack. The land enters the battlefield
    /// under `p`'s control and counts against the one-land-per-turn limit.
    fn play_land(&mut self, p: PlayerId, card: crate::ids::ObjId) {
        self.state.move_object(card, Zone::Battlefield, p);
        self.state.player_mut(p).lands_played_this_turn += 1;
        self.broadcast(GameEvent::ObjectMoved {
            obj: card,
            to: Zone::Battlefield,
        });
    }

    /// Resolve the top object of the stack (CR 608). Milestone 2 performs only the
    /// *structural* part — a permanent spell enters the battlefield, an instant/sorcery
    /// goes to its owner's graveyard, an ability ceases to exist (608.2n/608.3). Running
    /// the object's effect IR is the effect runtime's job (milestone 4). In a lands-only
    /// game the stack stays empty, so this is exercised only by unit tests.
    fn resolve_top(&mut self) {
        let Some(obj) = self.state.stack.pop() else {
            return;
        };
        match obj.kind {
            StackObjectKind::Spell(id) => {
                let owner = self.state.object(id).owner;
                let is_perm = self.state.object(id).chars.is_permanent();
                if is_perm {
                    self.state.move_object(id, Zone::Battlefield, obj.controller);
                    self.broadcast(GameEvent::ObjectMoved {
                        obj: id,
                        to: Zone::Battlefield,
                    });
                } else {
                    self.state.move_object(id, Zone::Graveyard, owner);
                    self.broadcast(GameEvent::ObjectMoved {
                        obj: id,
                        to: Zone::Graveyard,
                    });
                }
            }
            // An ability that has finished resolving simply ceases to exist (CR 608.2n).
            StackObjectKind::Ability => {}
        }
    }

    // ── the agenda pipeline (CR 117.5) ────────────────────────────────────────────────────

    /// Run the agenda to a fixpoint: recompute continuous effects → perform SBAs (loop
    /// until none) → put waiting triggers on the stack (APNAP) → repeat until stable.
    /// This is the law from WHITEBOARD_MODEL §2.2; run before any player receives priority.
    fn run_agenda(&mut self) {
        loop {
            // (1) Recompute layered characteristics if dirty — no-op until the layer
            // system arrives (chars/, milestone 5).
            self.recompute_continuous_if_dirty();

            // (2) State-based actions, performed as one event, repeated until none apply
            // (CR 704.3).
            let sbas = sba::collect(&self.state);
            if !sbas.is_empty() {
                self.perform_sbas(&sbas);
                if self.state.game_over {
                    return;
                }
                continue;
            }

            // (3) Put any waiting triggered abilities on the stack (CR 603.3), then loop:
            // doing so may itself enable new SBAs/triggers.
            let triggers = self.drain_pending_triggers();
            if !triggers.is_empty() {
                for t in triggers {
                    self.state.stack.push(t);
                }
                continue;
            }

            break; // game state stable → a player may receive priority
        }
    }

    fn recompute_continuous_if_dirty(&mut self) {
        // Milestone 5 (the layer system) lives here. Lands-only has no continuous effects.
    }

    /// Apply a batch of state-based actions simultaneously (CR 704.3). Milestone 2 handles
    /// only player losses; in two-player a loss ends the game (CR 104.2a).
    fn perform_sbas(&mut self, sbas: &[StateBasedAction]) {
        for sba in sbas {
            match sba {
                StateBasedAction::PlayerLoses { player, reason } => {
                    let pl = self.state.player_mut(*player);
                    if pl.has_lost {
                        continue;
                    }
                    pl.has_lost = true;
                    // Clear the decking flag so the SBA isn't re-collected forever.
                    if *reason == LossReason::DrewFromEmptyLibrary {
                        pl.drew_from_empty = false;
                    }
                }
            }
        }
        self.check_game_end();
    }

    /// Drain triggers waiting to go on the stack, APNAP-ordered (CR 603.3b): the active
    /// player's triggers first, then the others in turn order; each player's are kept in
    /// the order they were queued. Empty until the effect runtime arrives (M4).
    fn drain_pending_triggers(&mut self) -> Vec<crate::stack::StackObject> {
        if self.state.pending_triggers.is_empty() {
            return Vec::new();
        }
        let pending = std::mem::take(&mut self.state.pending_triggers);
        let order = self.turn_order();
        let mut ordered = Vec::with_capacity(pending.len());
        for seat in order {
            for t in pending.iter() {
                if t.controller == seat {
                    ordered.push(t.clone());
                }
            }
        }
        // Any whose controller isn't a current seat (shouldn't happen) appended last.
        for t in pending {
            if !ordered.iter().any(|o| o.id == t.id) {
                ordered.push(t);
            }
        }
        ordered
    }

    /// End the game if ≤1 player remains (CR 104.2a). The sole survivor wins.
    fn check_game_end(&mut self) {
        let living = self.state.living_players();
        if living.len() <= 1 {
            self.state.game_over = true;
            self.state.winner = living.first().copied();
            self.state.priority_player = None;
            let ev = GameEvent::GameEnded {
                winner: self.state.winner,
            };
            self.broadcast(ev);
        }
    }

    // ── primitives ────────────────────────────────────────────────────────────────────────

    /// Draw `count` cards for `p` from the top of their library (CR 120/121). A draw from an
    /// empty library sets the decking flag; the player loses on the next SBA check
    /// (CR 704.5b) — drawing-from-empty itself is not the loss.
    fn draw(&mut self, p: PlayerId, count: u32) {
        let mut drawn = 0;
        for _ in 0..count {
            let top = self.state.player_mut(p).library.pop();
            match top {
                Some(card) => {
                    if let Some(o) = self.state.objects.get_mut(&card) {
                        o.zone = Zone::Hand;
                    }
                    self.state.player_mut(p).hand.push(card);
                    drawn += 1;
                }
                None => {
                    self.state.player_mut(p).drew_from_empty = true;
                }
            }
        }
        if drawn > 0 {
            self.broadcast(GameEvent::DrewCards {
                player: p,
                count: drawn,
            });
        }
    }

    fn empty_mana_pools(&mut self) {
        for pl in &mut self.state.players {
            pl.mana_pool.amounts.clear();
        }
    }

    // ── the agent boundary ────────────────────────────────────────────────────────────────

    /// Ask seat `p` to decide `req`, presenting its information-filtered view. The single
    /// place the engine consults an agent for a choice.
    fn ask(&mut self, p: PlayerId, req: &DecisionRequest) -> DecisionResponse {
        let view = view_for(&self.state, p);
        self.agents[p.0 as usize].decide(&view, req)
    }

    /// Push a public event to every seat's `observe` channel (CR: the GRE diff stream).
    fn broadcast(&mut self, ev: GameEvent) {
        if self.record_events {
            self.event_log.push(ev.clone());
        }
        for seat in 0..self.state.players.len() {
            let pid = self.state.players[seat].id;
            let view = view_for(&self.state, pid);
            self.agents[seat].observe(&view, &ev);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::RandomAgent;
    use crate::basics::Zone;
    use crate::ids::{PlayerId, StackId};
    use crate::stack::{StackObject, StackObjectKind};
    use crate::state::{Characteristics, GameState};

    /// Build a two-player lands-only game: `lib` basic lands each, two `RandomAgent`s.
    fn lands_only_game(lib: usize, seed: u64) -> Engine {
        let mut state = GameState::new(2, seed);
        for seat in 0..2u32 {
            for _ in 0..lib {
                state.add_card(
                    PlayerId(seat),
                    Characteristics::basic_land("Forest"),
                    Zone::Library,
                );
            }
        }
        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(RandomAgent::new(seed ^ 0xA)),
            Box::new(RandomAgent::new(seed ^ 0xB)),
        ];
        Engine::new(state, agents)
    }

    #[test]
    fn lands_only_game_runs_to_completion_without_panic() {
        // Run many seeds; every game must terminate (someone decks) and conserve cards.
        for seed in 0..40u64 {
            let mut engine = lands_only_game(12, seed);
            let total_before: usize = engine.state.objects.len();
            let winner = engine.run_game();
            assert!(engine.state.game_over, "game must end (seed {seed})");
            // Card conservation: no object created or destroyed (lands-only).
            assert_eq!(engine.state.objects.len(), total_before, "seed {seed}");
            // Exactly one player should have decked out (the loser); the other won.
            let living = engine.state.living_players();
            assert_eq!(living.len(), 1, "two-player game has one survivor (seed {seed})");
            assert_eq!(winner, Some(living[0]), "winner is the survivor (seed {seed})");
        }
    }

    #[test]
    fn opening_hands_are_dealt_and_starting_player_skips_first_draw() {
        let mut engine = lands_only_game(30, 7);
        engine.start_game();
        // Seven-card opening hands (CR 103.5).
        assert_eq!(engine.state.player(PlayerId(0)).hand.len(), 7);
        assert_eq!(engine.state.player(PlayerId(1)).hand.len(), 7);

        // Run just the first turn's draw step: the starting player skips it (CR 103.8a).
        engine.begin_turn();
        engine.run_step(Phase::Untap);
        engine.run_step(Phase::Upkeep);
        engine.run_step(Phase::Draw);
        assert_eq!(
            engine.state.player(PlayerId(0)).hand.len(),
            7,
            "starting player skips the first draw"
        );
    }

    #[test]
    fn turn_advances_and_alternates_active_player() {
        let mut engine = lands_only_game(30, 3);
        engine.start_game();
        assert_eq!(engine.state.active_player, PlayerId(0));
        assert_eq!(engine.state.turn_number, 1);
        engine.take_turn();
        assert_eq!(engine.state.active_player, PlayerId(1));
        assert_eq!(engine.state.turn_number, 2);
        engine.take_turn();
        assert_eq!(engine.state.active_player, PlayerId(0));
        assert_eq!(engine.state.turn_number, 3);
    }

    #[test]
    fn decking_loses_via_state_based_action() {
        // Player 0 has an empty library; on their draw step they deck out and lose.
        let mut engine = lands_only_game(0, 1);
        // Give player 1 a card so they don't also deck immediately.
        engine.state.add_card(PlayerId(1), Characteristics::basic_land("Island"), Zone::Library);
        engine.started = true; // skip opening draws (libraries are tiny/empty)
        engine.run_game();
        assert!(engine.state.game_over);
        assert!(engine.state.player(PlayerId(0)).has_lost);
        assert_eq!(engine.state.winner, Some(PlayerId(1)));
    }

    #[test]
    fn stack_resolution_moves_a_permanent_spell_to_the_battlefield() {
        // Directly exercise resolve_top (a lands-only game never fills the stack).
        let mut engine = lands_only_game(0, 9);
        let card = engine.state.add_card(
            PlayerId(0),
            Characteristics::basic_land("Mountain"),
            Zone::Hand,
        );
        // Pretend it was cast: move to the stack zone + push a stack object.
        engine.state.objects.get_mut(&card).unwrap().zone = Zone::Stack;
        let pos = engine.state.player(PlayerId(0)).hand.iter().position(|&x| x == card).unwrap();
        engine.state.player_mut(PlayerId(0)).hand.remove(pos);
        let sid = engine.state.mint_stack();
        engine.state.stack.push(StackObject {
            id: sid,
            controller: PlayerId(0),
            source: Some(card),
            kind: StackObjectKind::Spell(card),
            targets: vec![],
        });
        assert_eq!(StackId(1), sid);
        engine.resolve_top();
        assert!(engine.state.stack.is_empty());
        assert_eq!(engine.state.object(card).zone, Zone::Battlefield);
        assert!(engine.state.player(PlayerId(0)).battlefield.contains(&card));
    }

    #[test]
    fn agenda_orders_sbas_before_triggers_and_reaches_fixpoint() {
        // With a lethal life total queued, the agenda must end the game (SBA), not hang.
        // (Library ≥ opening hand so the opening draw doesn't deck anyone.)
        let mut engine = lands_only_game(12, 2);
        engine.start_game();
        engine.state.player_mut(PlayerId(1)).life = 0;
        engine.run_agenda();
        assert!(engine.state.game_over);
        assert_eq!(engine.state.winner, Some(PlayerId(0)));
    }
}

/// Inline snapshot ("expect") tests for milestone-2 behaviour: the enumerated legal options
/// at a decision point (masking is the engine's job) and the CR-500s turn-structure trace.
#[cfg(test)]
mod expect_tests {
    use super::*;
    use crate::agent::{DecisionResponse, PlayerView};
    use crate::basics::{Phase, Zone};
    use crate::ids::PlayerId;
    use crate::state::{Characteristics, GameState};
    use expect_test::expect;

    /// An agent that always passes priority and declines/minimises every other choice — so a
    /// trace shows pure turn structure with no random land plays.
    struct PassAgent;
    impl Agent for PassAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::SelectCards { min, .. } => {
                    DecisionResponse::Indices((0..*min).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    fn pass_engine(state: GameState) -> Engine {
        Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)])
    }

    #[test]
    fn legal_priority_actions_are_enumerated_and_masked() {
        // Active player in their precombat main with two lands in hand, empty stack.
        let mut state = GameState::new(2, 1);
        state.add_card(PlayerId(0), Characteristics::basic_land("Forest"), Zone::Hand);
        state.add_card(PlayerId(0), Characteristics::basic_land("Island"), Zone::Hand);
        state.phase = Phase::PrecombatMain;
        let engine = pass_engine(state);

        // One PlayLand action per land in hand (CR 116.2a / 505.6).
        let actions = engine.legal_priority_actions(PlayerId(0));
        expect![[r#"
            [
                PlayLand {
                    card: ObjId(
                        1,
                    ),
                },
                PlayLand {
                    card: ObjId(
                        2,
                    ),
                },
            ]"#]]
        .assert_eq(&format!("{actions:#?}"));

        // Masking: the non-active player gets nothing at sorcery speed (CR 117.1a)…
        assert!(engine.legal_priority_actions(PlayerId(1)).is_empty());
        // …and after a land is played, the limit (CR 505.6b) removes the option.
        let mut state2 = GameState::new(2, 1);
        state2.add_card(PlayerId(0), Characteristics::basic_land("Forest"), Zone::Hand);
        state2.phase = Phase::PrecombatMain;
        state2.player_mut(PlayerId(0)).lands_played_this_turn = 1;
        assert!(pass_engine(state2).legal_priority_actions(PlayerId(0)).is_empty());
    }

    #[test]
    fn one_turn_walks_the_cr500_step_sequence() {
        // A single turn of pass/pass through every step, traced via PhaseBegan events.
        let mut state = GameState::new(2, 5);
        for _ in 0..10 {
            state.add_card(PlayerId(0), Characteristics::basic_land("Forest"), Zone::Library);
            state.add_card(PlayerId(1), Characteristics::basic_land("Forest"), Zone::Library);
        }
        let mut engine = pass_engine(state);
        engine.start_game();
        engine.record_events(true);
        engine.take_turn();

        let trace: String = engine
            .event_log
            .iter()
            .filter_map(|e| match e {
                GameEvent::PhaseBegan { turn, phase, active } => {
                    Some(format!("T{turn} {active:?} {phase:?}"))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        expect![[r#"
            T1 PlayerId(0) Untap
            T1 PlayerId(0) Upkeep
            T1 PlayerId(0) Draw
            T1 PlayerId(0) PrecombatMain
            T1 PlayerId(0) BeginCombat
            T1 PlayerId(0) DeclareAttackers
            T1 PlayerId(0) DeclareBlockers
            T1 PlayerId(0) CombatDamage
            T1 PlayerId(0) EndCombat
            T1 PlayerId(0) PostcombatMain
            T1 PlayerId(0) End
            T1 PlayerId(0) Cleanup"#]]
        .assert_eq(&trace);
    }
}
