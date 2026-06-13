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

use crate::agent::{
    AbilityRef, ActionRef, Agent, CastVariant, DecisionRequest, DecisionResponse, GameEvent,
    NumberReason, PlayableAction, PlayerView, SelectReason, StopStateView, TargetSlot,
};
use crate::basics::{CardType, CounterKind, ManaCost, Phase, Target, Zone, ZonePos};
use crate::effects::ability::{Ability, Cost, CostComponent, EventPattern, Keyword, Restriction, Timing};
use crate::effects::action::{Action, MoveCause, ResolutionCtx, Whiteboard, WbReason};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};
use crate::ids::{ObjId, PlayerId};
use crate::mana;
use crate::sba::{self, LossReason, StateBasedAction};
use crate::stack::{StackObject, StackObjectKind};
use crate::state::view::view_for;
use crate::state::GameState;
use crate::turn::{is_main_phase, step_grants_priority, TURN_STEPS};
use std::sync::{Arc, Mutex};

/// A hard cap on turns so a pathological game can never loop forever. Real games end far
/// sooner (a lands-only game ends when a player decks out, CR 704.5b). Reaching the cap
/// ends the game as a draw.
const MAX_TURNS: u32 = 2000;

/// Why the game ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// A player reached 0 or less life (CR 704.5a).
    ZeroLife,
    /// A player drew from an empty library (CR 704.5b).
    Decked,
    /// A player had ten or more poison counters (CR 704.5c).
    Poison,
    /// No winner: a draw, or the turn-cap was reached.
    DrawOrCapped,
}

/// The result of a finished game (a convenience read of the final state).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Outcome {
    pub winner: Option<PlayerId>,
    pub turns: u32,
    pub reason: EndReason,
}

/// A seat's MTGA-style "stops" configuration for the Arena-profile auto-pass policy
/// (AGENT_INTERFACE §8.1 decision *elision*). The engine still grants priority at every
/// window (CR-correct); this policy decides which windows the seat is actually *prompted* at.
/// Modeled on the recovered MTGA `SettingsMessage` (../mtga-re/docs/priority_stops.md).
#[derive(Debug, Clone)]
pub struct StopConfig {
    /// Arena-profile auto-pass on (a human/UI session) vs paper-CR every-window prompting
    /// (default, deterministic for differential-testing / RL replay). When off, every priority
    /// window prompts and the rest of the fields are inert.
    pub auto_pass: bool,
    /// "Full control": stop at every priority window (overrides everything else).
    pub full_control: bool,
    /// SmartStops (MTGA default ON): stop at any step where the seat has a legal play (so it
    /// never auto-passes past a chance to act). When OFF, the seat auto-passes through
    /// "unimportant" steps even with an action available.
    pub smart_stops: bool,
    /// `stackAutoPassOption == ResolveMyStackEffects` (MTGA default ON): while the seat's OWN
    /// object is on top of the stack, auto-pass so it resolves — don't re-prompt the seat to
    /// respond to its own spell/ability. OFF lets the seat respond to itself (like full
    /// control, but only over the stack).
    pub resolve_own_stack: bool,
    /// Per-step override of the Arena default, keyed by `(step, own_turn)` so the two sides of a
    /// step are independent (e.g. stop on *my* draw but not the opponent's). `own_turn` =
    /// `seat == active_player`. `Some(true)` = always stop here, `Some(false)` = never, absent =
    /// the Arena default.
    overrides: std::collections::BTreeMap<(Phase, bool), bool>,
}

impl Default for StopConfig {
    fn default() -> Self {
        StopConfig {
            auto_pass: false,        // paper-CR by default; a UI session turns it on
            full_control: false,
            smart_stops: true,       // MTGA default (smartStopsSetting = Enable)
            resolve_own_stack: true, // MTGA default (stackAutoPassOption = ResolveMyStackEffects)
            overrides: std::collections::BTreeMap::new(),
        }
    }
}

/// MTGA's `AutoPassOption` (../mtga-re/docs/priority_stops.md §5) — the named priority-passing
/// modes, exposed as a convenience that configures a seat's [`StopConfig`] flags. The engine
/// models the behaviourally-distinct knobs (full control, SmartStops, resolve-own-stack); the
/// finer turn-policy distinctions (Turn vs EndStep vs ResolveAll) are approximated onto them
/// and refined later against byte-exact captured defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoPassOption {
    /// Pass unless I have a legal action (SmartStops on).
    UnlessAction,
    /// Pass unless the opponent has acted (2-player ≈ `UnlessAction` + resolve-own-stack).
    UnlessOpponentAction,
    /// Default: auto-pass so my own stack objects resolve; re-stop on a stop/SmartStop or an
    /// opponent action.
    ResolveMyStackEffects,
    /// Auto-pass through the whole stack emptying (approximated: resolve-own-stack + no
    /// SmartStops while responding).
    ResolveAll,
    /// Stop at every priority window.
    FullControl,
}

impl StopConfig {
    /// Whether the seat stops (is prompted) at `step` on the given side of the turn
    /// (`own_turn` = it's the seat's own turn). `full_control` forces a stop; otherwise the
    /// per-side override, else the Arena default. This is the side-explicit primitive the UI
    /// reads for the phase bar without needing the live active player.
    pub fn stop_for(&self, step: Phase, own_turn: bool) -> bool {
        self.full_control
            || self
                .overrides
                .get(&(step, own_turn))
                .copied()
                .unwrap_or_else(|| arena_default_stop(step, own_turn))
    }

    /// Whether seat `p` stops at `step` given who is `active` (the live, turn-aware query).
    fn stops_at(&self, p: PlayerId, step: Phase, active: PlayerId) -> bool {
        self.stop_for(step, p == active)
    }

    /// Set/clear a per-side stop override (`own_turn` = the seat's own turn): `Some(true)` =
    /// always stop, `Some(false)` = never, `None` = revert to the Arena default. Public so a UI
    /// session holding an [`Engine::stops_handle`] can toggle one side of a stop mid-game (the
    /// engine re-reads the config at the next window).
    pub fn set_override(&mut self, step: Phase, own_turn: bool, stop: Option<bool>) {
        match stop {
            Some(v) => {
                self.overrides.insert((step, own_turn), v);
            }
            None => {
                self.overrides.remove(&(step, own_turn));
            }
        }
    }

    /// The effective stop state of each priority-granting step for *display* (the phase bar) on
    /// one side of the turn: `full_control || override || the persistent Arena default`. The UI
    /// calls this once per side (`own_turn = true` and `false`) to render both columns.
    pub fn effective_steps(&self, own_turn: bool) -> Vec<(Phase, bool)> {
        TURN_STEPS
            .iter()
            .copied()
            .filter(|&s| step_grants_priority(s))
            .map(|s| (s, self.stop_for(s, own_turn)))
            .collect()
    }
}

/// MTGA's persistent default stop set (../mtga-re/docs/priority_stops.md §1): only your own
/// two main phases. Declare-attackers/blockers are forced turn-based actions (always
/// presented), not priority stops; instant-speed windows are handled by SmartStops + the
/// no-action rule. (The lead's task listed combat-declare stops; decompile's recovered
/// behavior is MP1/MP2-only, which this matches.)
fn arena_default_stop(step: Phase, own_turn: bool) -> bool {
    matches!(step, Phase::PrecombatMain | Phase::PostcombatMain) && own_turn
}

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
    /// One [`StopConfig`] per seat (incl. its `auto_pass` flag), behind `Arc<Mutex<…>>` so a UI session can hold a live
    /// handle ([`Engine::stops_handle`]) and toggle a seat's stops *mid-game* from another
    /// thread; the engine re-reads the config at every priority window. RL/headless play never
    /// touches these (auto-pass stays off), so the lock is uncontended there.
    stops: Vec<Arc<Mutex<StopConfig>>>,
}

impl Engine {
    /// `agents` must have one entry per seat in `state`, in `PlayerId` order.
    pub fn new(state: GameState, agents: Vec<Box<dyn Agent>>) -> Self {
        assert_eq!(
            agents.len(),
            state.players.len(),
            "one agent per seat is required"
        );
        let stops = (0..state.players.len())
            .map(|_| Arc::new(Mutex::new(StopConfig::default())))
            .collect();
        Engine {
            state,
            agents,
            event_log: Vec::new(),
            record_events: false,
            started: false,
            stops,
        }
    }

    // ── Arena-profile auto-pass / stops (MTGA-style; AGENT_INTERFACE §8.1 elision) ───────────

    /// Enable/disable the Arena-profile auto-pass policy for *every* seat. Off (default) =
    /// paper-CR: every priority window prompts (deterministic for differential-testing / RL
    /// replay). On = a seat is prompted only at its stops + meaningful (non-priority) decisions.
    /// (The flag is per-seat in [`StopConfig`]; a UI can also flip a single seat's via its
    /// [`Engine::stops_handle`].)
    pub fn set_arena_auto_pass(&mut self, on: bool) {
        for cfg in &self.stops {
            cfg.lock().unwrap().auto_pass = on;
        }
    }
    /// "Full control" for a seat: stop at every priority window (overrides auto-pass).
    pub fn set_full_control(&mut self, p: PlayerId, on: bool) {
        self.stops[p.0 as usize].lock().unwrap().full_control = on;
    }
    /// SmartStops for a seat (MTGA default ON): stop wherever the seat has a legal play. When
    /// OFF, the seat auto-passes through unimportant steps even with an action.
    pub fn set_smart_stops(&mut self, p: PlayerId, on: bool) {
        self.stops[p.0 as usize].lock().unwrap().smart_stops = on;
    }
    /// `stackAutoPassOption` for a seat: ON (default) = auto-pass your own stack objects so
    /// they resolve; OFF = re-prompt you to respond to your own spell.
    pub fn set_resolve_own_stack(&mut self, p: PlayerId, on: bool) {
        self.stops[p.0 as usize].lock().unwrap().resolve_own_stack = on;
    }
    /// Apply a named MTGA [`AutoPassOption`] to a seat (a convenience over the flags).
    pub fn set_auto_pass_option(&mut self, p: PlayerId, opt: AutoPassOption) {
        let mut cfg = self.stops[p.0 as usize].lock().unwrap();
        match opt {
            AutoPassOption::FullControl => cfg.full_control = true,
            AutoPassOption::ResolveMyStackEffects | AutoPassOption::UnlessAction
            | AutoPassOption::UnlessOpponentAction => {
                cfg.full_control = false;
                cfg.smart_stops = true;
                cfg.resolve_own_stack = true;
            }
            AutoPassOption::ResolveAll => {
                cfg.full_control = false;
                cfg.smart_stops = false;
                cfg.resolve_own_stack = true;
            }
        }
    }
    /// Override a seat's stop at `step` on **both** sides of the turn (own + opponent's): a
    /// turn-agnostic stop. `Some(true)` = always stop, `Some(false)` = never, `None` = revert to
    /// the Arena default. (Back-compat convenience over [`Engine::set_stop_side`].)
    pub fn set_stop(&mut self, p: PlayerId, step: Phase, stop: Option<bool>) {
        let mut cfg = self.stops[p.0 as usize].lock().unwrap();
        cfg.set_override(step, true, stop);
        cfg.set_override(step, false, stop);
    }
    /// Override a seat's stop at `step` on a single side of the turn (`own_turn` = the seat's own
    /// turn) — e.g. stop on *my* draw but not the opponent's.
    pub fn set_stop_side(&mut self, p: PlayerId, step: Phase, own_turn: bool, stop: Option<bool>) {
        self.stops[p.0 as usize].lock().unwrap().set_override(step, own_turn, stop);
    }
    /// A live handle to a seat's stop configuration. A UI session holds the clone and mutates
    /// it (e.g. on a `SetStop` from the client) while the game runs on another thread; the
    /// engine consults the same `Mutex` at every priority window, so changes take effect at the
    /// next window with no game reset. (Arena auto-pass must be on for the config to matter.)
    pub fn stops_handle(&self, p: PlayerId) -> Arc<Mutex<StopConfig>> {
        Arc::clone(&self.stops[p.0 as usize])
    }
    /// A snapshot of a seat's stop configuration (for the UI).
    pub fn stop_config(&self, p: PlayerId) -> StopConfig {
        self.stops[p.0 as usize].lock().unwrap().clone()
    }
    /// Whether `p` would currently stop at `step` (for the UI to render active stops).
    pub fn is_stop(&self, p: PlayerId, step: Phase) -> bool {
        self.stops[p.0 as usize]
            .lock()
            .unwrap()
            .stops_at(p, step, self.state.active_player)
    }

    /// The Arena-profile decision: should `p`'s current priority window be auto-passed
    /// (elided) instead of prompting the agent? Forced/choice decisions never reach here —
    /// only the priority `Pass`/act window. Never auto-passes a stop or under full control;
    /// with auto-pass off, always prompts (returns false).
    fn should_auto_pass(&self, p: PlayerId, has_action: bool) -> bool {
        let cfg = self.stops[p.0 as usize].lock().unwrap();
        if !cfg.auto_pass {
            return false;
        }
        if cfg.full_control {
            return false; // stop at every window
        }
        // Stack-resolution policy (stackAutoPassOption). While something is on the stack the
        // window is about responding, not the step's stop.
        if let Some(top) = self.state.stack.top() {
            if top.controller == p && cfg.resolve_own_stack {
                return true; // ResolveMyStackEffects: let my own object resolve
            }
            // Opponent's object on top (or resolve-own-stack off): prompt to respond iff I can
            // act and SmartStops is on; otherwise auto-pass.
            return !(has_action && cfg.smart_stops);
        }
        // Stack empty → the step-based stop set.
        let step = self.state.phase;
        if cfg.stops_at(p, step, self.state.active_player) {
            return false; // a stop → prompt
        }
        if !has_action {
            return true; // nothing to do → auto-pass
        }
        // Has an action, not a stop. SmartStops (MTGA default) prompts wherever you can act; with
        // SmartStops OFF, "stop only at my explicit stops" — auto-pass EVERY non-stop empty-stack
        // window regardless of a castable action (per the recovered MTGA behavior / webui).
        !cfg.smart_stops
    }

    /// Enable recording of broadcast events into [`Engine::event_log`] (for tracing/tests).
    pub fn record_events(&mut self, on: bool) {
        self.record_events = on;
    }

    /// Skip the pre-game opening-hand deal: a later `run_game`/`start_game` will not shuffle
    /// or draw. Use this to play from a hand-built scenario (exact hands/board/libraries)
    /// without decking out on an empty library. (webui's scenario CLI / expect-tests.)
    pub fn skip_opening_deal(&mut self) {
        self.started = true;
    }

    /// The result of the game so far (meaningful once `game_over`). A convenience over
    /// reading `engine.state` directly.
    pub fn outcome(&self) -> Outcome {
        let reason = match self.state.end_reason {
            Some(crate::sba::LossReason::ZeroOrLessLife) => EndReason::ZeroLife,
            Some(crate::sba::LossReason::DrewFromEmptyLibrary) => EndReason::Decked,
            Some(crate::sba::LossReason::TenPoison) => EndReason::Poison,
            None => EndReason::DrawOrCapped,
        };
        Outcome {
            winner: self.state.winner,
            turns: self.state.turn_number,
            reason,
        }
    }

    /// The legal actions a player could take if they had priority right now (cast/play-land),
    /// already masked by the engine. Public so a UI can pre-render the options before a
    /// `Priority` decision arrives; the same list is delivered in the `Priority` request.
    pub fn legal_actions(&self, p: PlayerId) -> Vec<PlayableAction> {
        self.legal_priority_actions(p)
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

    /// Pre-game setup: shuffle libraries, draw opening hands, run the London mulligan (CR 103.5).
    /// Idempotent.
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
        self.run_mulligans(&seats, hand_size);
    }

    /// The London mulligan (CR 103.5). Each player may mulligan any number of times; a mulligan
    /// shuffles their hand into their library and draws a fresh seven. After keeping, a player
    /// puts one card on the bottom of their library for each mulligan they took (CR 103.5c).
    /// Run in rounds in turn order (starting player first) so decisions interleave as the rules
    /// describe; since hands are hidden, this is equivalent to the simultaneous procedure.
    ///
    /// Decisions flow through the normal `Agent` boundary (`Mulligan` → `Bool(true)`=mulligan,
    /// then `SelectCards{BottomForMulligan}` on keep). `RandomAgent` keeps every hand, so this is
    /// a no-op for random self-play and existing deterministic tests; a scripted/human/RL agent
    /// drives real mulligans.
    fn run_mulligans(&mut self, seats: &[PlayerId], hand_size: u32) {
        // A 7th mulligan would bottom the entire hand — treat it as a forced keep.
        const MAX_MULLIGANS: u32 = 7;
        // Turn order starting from the active (starting) player.
        let order: Vec<PlayerId> = {
            let n = seats.len();
            let s = seats
                .iter()
                .position(|&p| p == self.state.active_player)
                .unwrap_or(0);
            (0..n).map(|i| seats[(s + i) % n]).collect()
        };
        let mut kept = vec![false; seats.len()];
        let mut mulls = vec![0u32; seats.len()];

        loop {
            let mut progressed = false;
            for &p in &order {
                let i = p.0 as usize;
                if kept[i] {
                    continue;
                }
                let hand = self.state.player(p).hand.clone();
                let req = DecisionRequest::Mulligan {
                    hand,
                    mulligans_taken: mulls[i],
                    will_bottom_if_kept: mulls[i].min(hand_size),
                };
                let wants_mulligan = mulls[i] < MAX_MULLIGANS
                    && matches!(self.ask(p, &req), DecisionResponse::Bool(true));
                if wants_mulligan {
                    let hand_ids = self.state.player(p).hand.clone();
                    for id in hand_ids {
                        self.state.move_object(id, Zone::Library, p);
                    }
                    self.state.shuffle_library(p);
                    self.draw(p, hand_size);
                    mulls[i] += 1;
                    progressed = true;
                } else {
                    kept[i] = true;
                }
            }
            if kept.iter().all(|&k| k) || !progressed {
                break;
            }
        }

        // Bottoming (CR 103.5c), in turn order: one card per mulligan taken.
        for &p in &order {
            let i = p.0 as usize;
            let n = mulls[i].min(hand_size);
            if n == 0 {
                continue;
            }
            let hand = self.state.player(p).hand.clone();
            let req = DecisionRequest::SelectCards {
                reason: SelectReason::BottomForMulligan,
                from: hand.clone(),
                min: n,
                max: n,
                description: format!("Put {n} card(s) on the bottom of your library."),
            };
            let chosen = match self.ask(p, &req) {
                DecisionResponse::Indices(ix) => ix.into_iter().map(|x| x as usize).collect(),
                _ => Vec::new(),
            };
            // Validate: distinct, in range; top up to exactly `n` if the agent under-selected.
            let mut seen = std::collections::BTreeSet::new();
            let mut picks: Vec<usize> = chosen
                .into_iter()
                .filter(|&x| x < hand.len() && seen.insert(x))
                .take(n as usize)
                .collect();
            for x in 0..hand.len() {
                if picks.len() == n as usize {
                    break;
                }
                if seen.insert(x) {
                    picks.push(x);
                }
            }
            // Put them on the bottom of the library (front of the vec — `draw` pops the end = top).
            for id in picks.into_iter().map(|x| hand[x]).collect::<Vec<_>>() {
                if let Some(pos) = self.state.player_mut(p).hand.iter().position(|&h| h == id) {
                    self.state.player_mut(p).hand.remove(pos);
                }
                if let Some(o) = self.state.objects.get_mut(&id) {
                    o.zone = Zone::Library;
                }
                self.state.player_mut(p).library.insert(0, id);
            }
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
                o.used_once_per_turn = false; // loyalty abilities are usable again (CR 606.3)
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
            // Combat turn-based actions (CR 508/509/510/511 — see combat/).
            Phase::DeclareAttackers => self.declare_attackers(),
            Phase::DeclareBlockers => self.declare_blockers(),
            Phase::CombatDamage => self.combat_damage(),
            Phase::EndCombat => self.end_combat(),
            // Untap/Upkeep/Begin/main phases/End have no further turn-based actions here.
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
                o.dealt_deathtouch = false;
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
            // Arena-profile auto-pass (AGENT_INTERFACE §8.1): elide this window (treat as a
            // pass without prompting the agent) when the policy says so. Off ⇒ always prompt.
            let response = if self.should_auto_pass(p, !actions.is_empty()) {
                DecisionResponse::Pass
            } else {
                let req = DecisionRequest::Priority {
                    actions: actions.clone(),
                    can_pass: true,
                };
                self.ask(p, &req)
            };
            match response {
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
    /// masking, CR 117): play a land (CR 116.2a) and cast a spell (CR 601), at the right
    /// timing and only if affordable + (if it targets) has a legal target.
    fn legal_priority_actions(&self, p: PlayerId) -> Vec<PlayableAction> {
        let mut actions = Vec::new();
        let s = &self.state;
        let sorcery_speed = p == s.active_player && is_main_phase(s.phase) && s.stack.is_empty();

        // Play a land (CR 116.2a / 505.6b: one per turn, main phase, empty stack, your turn).
        if sorcery_speed && s.player(p).lands_played_this_turn < 1 {
            for &card in &s.player(p).hand {
                if s.object(card).chars.is_land() {
                    actions.push(PlayableAction::PlayLand { card });
                }
            }
        }

        // Cast a spell (CR 601). Instants any time you have priority; everything else at
        // sorcery speed (CR 117.1a).
        for &card in &s.player(p).hand {
            let chars = &s.object(card).chars;
            if chars.is_land() {
                continue;
            }
            let cost = match &chars.mana_cost {
                Some(c) => c,
                None => continue,
            };
            // Instants and Flash (CR 702.8) cast at instant speed; everything else sorcery-speed.
            let instant_speed = chars.has_type(CardType::Instant) || chars.keywords.contains(&Keyword::Flash);
            let timing_ok = instant_speed || sorcery_speed;
            if !timing_ok || !mana::can_pay(s, p, cost) {
                continue;
            }
            // Must have a legal target for each "target" the spell requires (CR 601.2c).
            let has_targets = match s.def_of(card).and_then(|d| d.spell_effect()) {
                Some(eff) => collect_target_specs(eff)
                    .iter()
                    .all(|spec| self.target_candidates(spec, p).len() as u32 >= spec.min.max(1)),
                None => true,
            };
            if has_targets {
                actions.push(PlayableAction::Cast {
                    spell: card,
                    variant: CastVariant::Normal,
                });
            }
        }

        // Activated abilities (CR 602) of permanents `p` controls — e.g. Equip. Mana abilities
        // (CR 605) use a separate no-stack path and are skipped here. Masked by timing, the
        // activation restriction, cost payability, and (if it targets) a legal target.
        for &perm in &s.player(p).battlefield {
            let Some(def) = s.def_of(perm) else { continue };
            for (i, ab) in def.abilities.iter().enumerate() {
                let Ability::Activated { cost, effect, timing, restriction, is_mana } = ab else {
                    continue;
                };
                if *is_mana {
                    continue;
                }
                let timing_ok = match timing {
                    Timing::Instant => true,
                    Timing::Sorcery => sorcery_speed,
                };
                if !timing_ok {
                    continue;
                }
                if matches!(restriction, Some(Restriction::OnlyYourTurn)) && p != s.active_player {
                    continue;
                }
                // Once-per-turn (CR 606.3, loyalty abilities): per planeswalker, across all its
                // loyalty abilities — tracked by `Object.used_once_per_turn`.
                if matches!(restriction, Some(Restriction::OncePerTurn)) && s.object(perm).used_once_per_turn {
                    continue;
                }
                if !self.can_pay_cost(p, perm, cost) {
                    continue;
                }
                let has_targets = collect_target_specs(effect)
                    .iter()
                    .all(|spec| self.target_candidates(spec, p).len() as u32 >= spec.min.max(1));
                if has_targets {
                    actions.push(PlayableAction::Activate {
                        source: perm,
                        ability: AbilityRef(i as u32),
                    });
                }
            }
        }
        actions
    }

    /// Whether `p` can pay `cost` to activate an ability of `source`. Handles the components the
    /// starter set uses (mana, `{T}`); other components aren't masked yet and pass through.
    fn can_pay_cost(&self, p: PlayerId, source: ObjId, cost: &Cost) -> bool {
        if let Some(m) = &cost.mana {
            if !mana::can_pay(&self.state, p, m) {
                return false;
            }
        }
        for c in &cost.components {
            let ok = match c {
                CostComponent::TapSelf => {
                    self.state.objects.get(&source).is_some_and(|o| !o.status.tapped)
                }
                // Loyalty (CR 606.2): `+N`/`0` always payable; `−N` only if loyalty ≥ N.
                CostComponent::Loyalty(n) => {
                    *n >= 0
                        || self.state.objects.get(&source).is_some_and(|o| {
                            o.counters.get(&CounterKind::Loyalty) as i32 >= -*n
                        })
                }
                _ => true,
            };
            if !ok {
                return false;
            }
        }
        true
    }

    fn perform_priority_action(&mut self, p: PlayerId, action: &PlayableAction) {
        match action {
            PlayableAction::PlayLand { card } => self.play_land(p, *card),
            PlayableAction::Cast { spell, .. } => self.cast_spell(p, *spell),
            PlayableAction::Activate { source, ability } => {
                self.activate_ability(p, *source, *ability)
            }
            // ActivateMana / Special: separate paths (mana abilities don't use the stack).
            _ => {}
        }
    }

    /// Activate a (non-mana) activated ability (CR 602.2): put it on the stack, choose targets
    /// (locked now, 602.2b), then pay the cost. It resolves via [`Engine::resolve_top`].
    fn activate_ability(&mut self, p: PlayerId, source: ObjId, ability: AbilityRef) {
        let idx = ability.0 as usize;
        let (cost, effect, restriction) =
            match self.state.def_of(source).and_then(|d| d.abilities.get(idx)) {
                Some(Ability::Activated { cost, effect, restriction, is_mana: false, .. }) => {
                    (cost.clone(), effect.clone(), restriction.clone())
                }
                _ => return,
            };
        let sid = self.state.mint_stack();
        self.state.stack.push(StackObject {
            id: sid,
            controller: p,
            source: Some(source),
            kind: StackObjectKind::Ability { index: idx as u32 },
            targets: Vec::new(),
            x: None,
        });
        let specs = collect_target_specs(&effect);
        if !specs.is_empty() {
            let slots: Vec<TargetSlot> = specs
                .iter()
                .map(|spec| TargetSlot {
                    description: String::new(),
                    legal: self.target_candidates(spec, p),
                    min: spec.min,
                    max: spec.max,
                })
                .collect();
            let req = DecisionRequest::ChooseTargets {
                for_action: ActionRef(sid),
                slots: slots.clone(),
            };
            let resp = self.ask(p, &req);
            let chosen = parse_targets(&slots, &resp);
            if let Some(obj) = self.state.stack.items.iter_mut().find(|s| s.id == sid) {
                obj.targets = chosen;
            }
        }
        self.pay_cost(p, source, &cost);
        // Mark the once-per-turn limit (CR 606.3) as used on this permanent.
        if matches!(restriction, Some(Restriction::OncePerTurn)) {
            if let Some(o) = self.state.objects.get_mut(&source) {
                o.used_once_per_turn = true;
            }
        }
    }

    /// Pay an ability/cost's components (CR 118). Mana is auto-tapped; the starter set also uses
    /// `{T}`. Components beyond these aren't charged yet (none in the starter pool need them).
    fn pay_cost(&mut self, p: PlayerId, source: ObjId, cost: &Cost) {
        if let Some(m) = &cost.mana {
            mana::auto_pay(&mut self.state, p, m);
        }
        for c in &cost.components {
            match c {
                CostComponent::TapSelf => {
                    if let Some(o) = self.state.objects.get_mut(&source) {
                        o.status.tapped = true;
                    }
                }
                CostComponent::Loyalty(n) => {
                    if let Some(o) = self.state.objects.get_mut(&source) {
                        let cur = o.counters.counts.entry(CounterKind::Loyalty).or_insert(0);
                        *cur = (*cur as i32 + n).max(0) as u32;
                    }
                }
                _ => {}
            }
        }
    }

    /// Play a land: a special action (CR 116.2a), no stack. Routed through the whiteboard so
    /// ETB replacement effects (e.g. Root Maze "lands enter tapped") apply and the ETB event
    /// fires from commit. Counts against the one-land-per-turn limit.
    fn play_land(&mut self, p: PlayerId, card: ObjId) {
        let ctx = ResolutionCtx {
            controller: Some(p),
            source: Some(card),
            ..Default::default()
        };
        let mut wb = Whiteboard::new(WbReason::TurnBased, ctx);
        wb.push(Action::MoveZone {
            obj: card,
            to: Zone::Battlefield,
            pos: ZonePos::Any,
            cause: MoveCause::Other,
        });
        self.commit(wb);
        self.state.player_mut(p).lands_played_this_turn += 1;
    }

    /// Cast a spell from `p`'s hand (CR 601, minimal): put it on the stack (601.2a), choose
    /// targets (601.2c), auto-pay its mana cost (601.2f–h), and announce it cast (601.2i).
    /// Affordability + target availability are pre-checked in `legal_priority_actions`, so no
    /// rewind (CR 732) is needed. The caller keeps priority with the caster (CR 601.2i).
    fn cast_spell(&mut self, p: PlayerId, card: ObjId) {
        let cost = match self.state.object(card).chars.mana_cost.clone() {
            Some(c) => c,
            None => return,
        };
        let effect = self.state.def_of(card).and_then(|d| d.spell_effect().cloned());
        let mut specs = effect.as_ref().map(collect_target_specs).unwrap_or_default();
        // An Aura spell targets the permanent it will enchant (CR 601.2c / 303.4f); it has no
        // spell ability, so the target is structural. First pass: Auras enchant a creature
        // (matches the starter set's "Enchant creature"); a general enchant restriction is future.
        if self.is_aura(card) {
            specs.push(TargetSpec {
                kind: TargetKind::Creature(crate::effects::target::CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            });
        }

        // 601.2a: the card becomes a spell on top of the stack.
        let sid = self.state.mint_stack();
        self.move_to_stack(card, p);
        // C10 / 601.2b: choose X if the cost has `{X}` (bounded by affordable mana).
        let chosen_x = if cost.x > 0 {
            let avail = mana::available_mana(&self.state, p);
            let fixed = cost.generic + cost.colored.values().sum::<u32>();
            let max_x = avail.saturating_sub(fixed) / cost.x;
            let resp = self.ask(
                p,
                &DecisionRequest::ChooseNumber {
                    reason: NumberReason::ChooseX,
                    min: 0,
                    max: max_x as i64,
                    step: 1,
                    forbidden: Vec::new(),
                    disallow_even: false,
                    disallow_odd: false,
                },
            );
            match resp {
                DecisionResponse::Number(n) => n.clamp(0, max_x as i64) as u32,
                _ => 0,
            }
        } else {
            0
        };
        self.state.stack.push(StackObject {
            id: sid,
            controller: p,
            source: Some(card),
            kind: StackObjectKind::Spell(card),
            targets: Vec::new(),
            x: if cost.x > 0 { Some(chosen_x) } else { None },
        });

        // 601.2c: choose targets (locked now).
        if !specs.is_empty() {
            let slots: Vec<TargetSlot> = specs
                .iter()
                .map(|spec| TargetSlot {
                    description: String::new(),
                    legal: self.target_candidates(spec, p),
                    min: spec.min,
                    max: spec.max,
                })
                .collect();
            let req = DecisionRequest::ChooseTargets {
                for_action: ActionRef(sid),
                slots: slots.clone(),
            };
            let resp = self.ask(p, &req);
            let chosen = parse_targets(&slots, &resp);
            if let Some(obj) = self.state.stack.items.iter_mut().find(|s| s.id == sid) {
                obj.targets = chosen;
            }
        }

        // 601.2f–h: pay the total cost (auto-tap lands), with {X} settled to `chosen_x`.
        let pay = ManaCost {
            generic: cost.generic + chosen_x * cost.x,
            colored: cost.colored.clone(),
            x: 0,
        };
        mana::auto_pay(&mut self.state, p, &pay);

        // 601.2i: the spell has been cast.
        self.broadcast(GameEvent::SpellCast {
            spell: sid,
            controller: p,
        });
    }

    /// Move a card from its owner's hand onto the stack zone (the object's `ObjId` is kept;
    /// the [`StackObject`] wraps it with a `StackId`).
    fn move_to_stack(&mut self, card: ObjId, controller: PlayerId) {
        let owner = self.state.object(card).owner;
        let hand = &mut self.state.player_mut(owner).hand;
        if let Some(pos) = hand.iter().position(|&x| x == card) {
            hand.remove(pos);
        }
        if let Some(o) = self.state.objects.get_mut(&card) {
            o.zone = Zone::Stack;
            o.controller = controller;
        }
    }

    /// The legal target candidates for one target spec (the engine pre-filters; masking is
    /// the engine's job). Milestone 3 supports "any target" (CR 115.4) and player/creature.
    fn target_candidates(&self, spec: &TargetSpec, caster: PlayerId) -> Vec<Target> {
        let creatures = || {
            self.state
                .objects
                .values()
                .filter(|o| {
                    o.zone == Zone::Battlefield
                        && self.state.computed(o.id).is_creature()
                        && self.targetable_by(o.id, caster)
                })
                .map(|o| Target::Object(o.id))
        };
        let players = || {
            self.state
                .players
                .iter()
                .filter(|p| !p.has_lost)
                .map(|p| Target::Player(p.id))
        };
        match &spec.kind {
            TargetKind::Any => creatures().chain(players()).collect(),
            TargetKind::Player => players().collect(),
            TargetKind::Creature(filter) | TargetKind::Permanent(filter) => creatures()
                .filter(|t| self.target_matches_filter(t, filter, caster))
                .collect(),
            // StackObject / CardInZone: not needed by the starter set.
            _ => Vec::new(),
        }
    }

    /// Apply the subset of a `CardFilter` that targeting needs (the engine still pre-masks
    /// hexproof in `targetable_by`). Currently honours `ControlledBy` (e.g. equip's "creature
    /// you control"); other predicates aren't yet enforced at target time and pass through.
    fn target_matches_filter(&self, t: &Target, filter: &CardFilter, caster: PlayerId) -> bool {
        let Target::Object(id) = t else { return true };
        let Some(o) = self.state.objects.get(id) else {
            return false;
        };
        match filter {
            CardFilter::ControlledBy(PlayerRef::Controller | PlayerRef::Owner) => {
                o.controller == caster
            }
            CardFilter::ControlledBy(PlayerRef::Opponent | PlayerRef::EachOpponent) => {
                o.controller != caster
            }
            CardFilter::All(fs) => fs.iter().all(|f| self.target_matches_filter(t, f, caster)),
            CardFilter::AnyOf(fs) => fs.iter().any(|f| self.target_matches_filter(t, f, caster)),
            CardFilter::Not(f) => !self.target_matches_filter(t, f, caster),
            _ => true,
        }
    }

    /// Whether `obj` may be targeted by a spell/ability `caster` controls (CR 115 + hexproof,
    /// CR 702.11): hexproof can't be targeted by the controller's opponents. (Shroud/ward are
    /// deferred — niche / need a cost.)
    fn targetable_by(&self, obj: ObjId, caster: PlayerId) -> bool {
        let Some(o) = self.state.objects.get(&obj) else {
            return false;
        };
        if self.state.computed(obj).has_keyword(Keyword::Hexproof) && o.controller != caster {
            return false;
        }
        true
    }

    /// Whether `id` is an Aura (the enchantment subtype, CR 303). Auras have no spell ability;
    /// their cast-target and enters-attached behaviour is wired structurally on this flag.
    fn is_aura(&self, id: ObjId) -> bool {
        self.state
            .objects
            .get(&id)
            .is_some_and(|o| o.chars.subtypes.iter().any(|s| s == "Aura"))
    }

    /// CR 608.2b: a spell/ability resolves unless *every* target is illegal. (Returns true if
    /// it has no targets.)
    fn targets_still_legal(&self, targets: &[Target]) -> bool {
        targets.is_empty() || targets.iter().any(|t| self.target_legal(t))
    }

    fn target_legal(&self, t: &Target) -> bool {
        match t {
            Target::Player(p) => self
                .state
                .players
                .get(p.0 as usize)
                .is_some_and(|pl| !pl.has_lost),
            Target::Object(o) => self
                .state
                .objects
                .get(o)
                .is_some_and(|x| x.zone == Zone::Battlefield),
            Target::Stack(_) => false,
        }
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
                    // An Aura spell whose target became illegal doesn't resolve — it's put into
                    // its owner's graveyard (CR 608.3b / 702.3 — no enters-attached).
                    if self.is_aura(id) && !self.targets_still_legal(&obj.targets) {
                        self.state.move_object(id, Zone::Graveyard, owner);
                        self.broadcast(GameEvent::ObjectMoved { obj: id, to: Zone::Graveyard });
                        return;
                    }
                    // Permanent spell → enters the battlefield (CR 608.3), routed through the
                    // whiteboard so ETB replacement effects (enters-with-counters / -tapped)
                    // apply and the ETB event (→ triggers) fires from commit.
                    let ctx = ResolutionCtx {
                        controller: Some(obj.controller),
                        source: Some(id),
                        ..Default::default()
                    };
                    let mut wb = Whiteboard::new(WbReason::Resolve(obj.id), ctx);
                    wb.push(Action::MoveZone {
                        obj: id,
                        to: Zone::Battlefield,
                        pos: ZonePos::Any,
                        cause: MoveCause::Resolved,
                    });
                    self.commit(wb);
                    // An Aura enters the battlefield attached to its chosen target (CR 303.4f /
                    // 608.3e). Set the link after the ETB commit, then mark chars dirty so the
                    // "enchanted creature …" static (AttachedHost) takes effect.
                    if self.is_aura(id) {
                        if let Some(Target::Object(host)) = obj.targets.first().copied() {
                            if let Some(o) = self.state.objects.get_mut(&id) {
                                o.attached_to = Some(host);
                            }
                            self.state.mark_chars_dirty();
                        }
                    }
                } else {
                    // Instant/sorcery: recheck targets (608.2b), run the effect (608.2c),
                    // then put it into its owner's graveyard (608.2n).
                    let effect = self.state.def_of(id).and_then(|d| d.spell_effect().cloned());
                    if let Some(effect) = effect {
                        if self.targets_still_legal(&obj.targets) {
                            let ctx = ResolutionCtx {
                                controller: Some(obj.controller),
                                source: Some(id),
                                x: obj.x,
                                chosen_targets: obj.targets.clone(),
                                chosen_modes: Vec::new(),
                            };
                            self.resolve_effect(&effect, &ctx, WbReason::Resolve(obj.id));
                        }
                        // else: all targets illegal ⇒ countered by game rules, no effect.
                    }
                    self.state.move_object(id, Zone::Graveyard, owner);
                    self.broadcast(GameEvent::ObjectMoved {
                        obj: id,
                        to: Zone::Graveyard,
                    });
                }
            }
            StackObjectKind::Ability { index } => {
                // A triggered or activated ability on the stack: run its effect, then it ceases
                // to exist (CR 608.2n). The effect is looked up from the source's CardDef by
                // `grp_id` (persists across zones, so dies-triggers resolve too).
                let effect = obj.source.and_then(|src| {
                    self.state.def_of(src).and_then(|d| match d.abilities.get(index as usize) {
                        Some(Ability::Triggered { effect, .. })
                        | Some(Ability::Activated { effect, .. }) => Some(effect.clone()),
                        _ => None,
                    })
                });
                if let (Some(effect), Some(src)) = (effect, obj.source) {
                    if self.targets_still_legal(&obj.targets) {
                        let ctx = ResolutionCtx {
                            controller: Some(obj.controller),
                            source: Some(src),
                            x: None,
                            chosen_targets: obj.targets.clone(),
                            chosen_modes: Vec::new(),
                        };
                        self.resolve_effect(&effect, &ctx, WbReason::Resolve(obj.id));
                    }
                }
            }
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
                    self.put_trigger_on_stack(t);
                }
                continue;
            }

            break; // game state stable → a player may receive priority
        }
    }

    fn recompute_continuous_if_dirty(&mut self) {
        // CR 613.5 / WHITEBOARD_MODEL §2.4: rebuild the layer-system characteristics cache
        // when a dirty signal has fired (zone/counter/ability/timestamp change), before any
        // SBA/trigger check or player decision reads computed characteristics.
        if self.state.chars_is_dirty() {
            self.state.recompute_continuous();
        }
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
                    // Record the first loss reason for the game's Outcome.
                    if self.state.end_reason.is_none() {
                        self.state.end_reason = Some(*reason);
                    }
                }
                StateBasedAction::CreatureDies { creature, .. } => {
                    let owner = match self.state.objects.get(creature) {
                        Some(o) if o.zone == Zone::Battlefield => o.owner,
                        _ => continue,
                    };
                    if self.state.move_object(*creature, Zone::Graveyard, owner) {
                        self.broadcast(GameEvent::PermanentDied { obj: *creature });
                        self.broadcast(GameEvent::ObjectMoved {
                            obj: *creature,
                            to: Zone::Graveyard,
                        });
                    }
                }
                StateBasedAction::AuraFallsOff { aura } => {
                    let owner = match self.state.objects.get(aura) {
                        Some(o) if o.zone == Zone::Battlefield => o.owner,
                        _ => continue,
                    };
                    if self.state.move_object(*aura, Zone::Graveyard, owner) {
                        self.broadcast(GameEvent::ObjectMoved {
                            obj: *aura,
                            to: Zone::Graveyard,
                        });
                    }
                }
                StateBasedAction::EquipmentUnattaches { equipment } => {
                    if let Some(o) = self.state.objects.get_mut(equipment) {
                        o.attached_to = None;
                    }
                    self.state.mark_chars_dirty();
                }
                StateBasedAction::PlaneswalkerDies { pw } => {
                    let owner = match self.state.objects.get(pw) {
                        Some(o) if o.zone == Zone::Battlefield => o.owner,
                        _ => continue,
                    };
                    if self.state.move_object(*pw, Zone::Graveyard, owner) {
                        self.broadcast(GameEvent::PermanentDied { obj: *pw });
                        self.broadcast(GameEvent::ObjectMoved {
                            obj: *pw,
                            to: Zone::Graveyard,
                        });
                    }
                }
            }
        }
        self.check_game_end();
    }

    /// Put a triggered ability on the stack, choosing its targets now if it targets
    /// (CR 603.3d). A trigger that requires a target but has none is removed (not put on the
    /// stack, CR 603.3c).
    fn put_trigger_on_stack(&mut self, mut t: StackObject) {
        let effect = match (t.source, &t.kind) {
            (Some(src), StackObjectKind::Ability { index }) => {
                self.state.def_of(src).and_then(|d| match d.abilities.get(*index as usize) {
                    Some(Ability::Triggered { effect, .. }) => Some(effect.clone()),
                    _ => None,
                })
            }
            _ => None,
        };
        if let Some(effect) = effect {
            let specs = collect_target_specs(&effect);
            if !specs.is_empty() {
                let slots: Vec<TargetSlot> = specs
                    .iter()
                    .map(|spec| TargetSlot {
                        description: String::new(),
                        legal: self.target_candidates(spec, t.controller),
                        min: spec.min,
                        max: spec.max,
                    })
                    .collect();
                // No legal target for a required slot ⇒ the trigger is removed (CR 603.3c).
                if slots.iter().any(|s| s.min > 0 && s.legal.is_empty()) {
                    return;
                }
                let req = DecisionRequest::ChooseTargets {
                    for_action: ActionRef(t.id),
                    slots: slots.clone(),
                };
                let resp = self.ask(t.controller, &req);
                t.targets = parse_targets(&slots, &resp);
            }
        }
        self.state.stack.push(t);
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
    pub(crate) fn draw(&mut self, p: PlayerId, count: u32) {
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
    pub(crate) fn ask(&mut self, p: PlayerId, req: &DecisionRequest) -> DecisionResponse {
        let view = self.view_for_seat(p);
        self.agents[p.0 as usize].decide(&view, req)
    }

    /// The info-filtered [`PlayerView`] for `p`, augmented with the seat's Arena stop state
    /// (the settings-echo — `PlayerView.stops`; `None` when the auto-pass profile is off).
    /// Used everywhere the engine builds a view (decide/observe).
    fn view_for_seat(&self, p: PlayerId) -> PlayerView {
        let mut view = view_for(&self.state, p);
        // Snapshot the config once (don't hold the lock while computing per-step, which would
        // re-enter the same non-reentrant Mutex via `stops_at`).
        let cfg = self.stops[p.0 as usize].lock().unwrap().clone();
        if cfg.auto_pass {
            let active = self.state.active_player;
            let per_step = TURN_STEPS
                .iter()
                .copied()
                .filter(|&s| step_grants_priority(s))
                .map(|s| (s, cfg.stops_at(p, s, active)))
                .collect();
            view.stops = Some(StopStateView {
                full_control: cfg.full_control,
                smart_stops: cfg.smart_stops,
                resolve_own_stack: cfg.resolve_own_stack,
                per_step,
            });
        }
        view
    }

    /// Push a public event to every seat's `observe` channel (CR: the GRE diff stream), and
    /// collect any triggered abilities that watch this event (CR 603.2).
    pub(crate) fn broadcast(&mut self, ev: GameEvent) {
        if self.record_events {
            self.event_log.push(ev.clone());
        }
        for seat in 0..self.state.players.len() {
            let pid = self.state.players[seat].id;
            let view = self.view_for_seat(pid);
            self.agents[seat].observe(&view, &ev);
        }
        self.collect_triggers(&ev);
    }

    /// Scan for triggered abilities whose pattern matches `ev` and queue them (CR 603.2/603.3):
    /// they go on the stack the next time a player would get priority (the agenda loop drains
    /// `pending_triggers`). Milestone-4 prototype handles `SelfEnters` and `SelfDies`.
    fn collect_triggers(&mut self, ev: &GameEvent) {
        match ev {
            GameEvent::ObjectMoved { obj, to: Zone::Battlefield } => {
                // The entering object's own ETB triggers (CR 603.6a)…
                self.queue_self_triggers(*obj, EventPattern::SelfEnters);
                // …plus every other permanent watching "a [filter] enters" (CR 603.2), e.g.
                // landfall = PermanentEnters(a land you control). C4.
                self.queue_watching_enters_triggers(*obj);
            }
            GameEvent::PermanentDied { obj } => {
                self.queue_self_triggers(*obj, EventPattern::SelfDies);
            }
            _ => {}
        }
    }

    /// Queue `subject`'s own triggered abilities matching `want` (the Self* patterns).
    fn queue_self_triggers(&mut self, subject: ObjId, want: EventPattern) {
        let Some(def) = self.state.def_of(subject) else {
            return;
        };
        let matches: Vec<u32> = def
            .abilities
            .iter()
            .enumerate()
            .filter(|(_, a)| matches!(a, Ability::Triggered { event, .. } if *event == want))
            .map(|(i, _)| i as u32)
            .collect();
        if matches.is_empty() {
            return;
        }
        let controller = self.state.object(subject).controller;
        for index in matches {
            let id = self.state.mint_stack();
            self.state.pending_triggers.push(StackObject {
                id,
                controller,
                source: Some(subject),
                kind: StackObjectKind::Ability { index },
                targets: Vec::new(),
                x: None,
            });
        }
    }

    /// Queue every battlefield permanent's `PermanentEnters(filter)` trigger that matches the
    /// just-entered object (CR 603.2). The filter is evaluated relative to the WATCHER's
    /// controller, so "a land you control enters" (landfall) means the watcher's controller.
    fn queue_watching_enters_triggers(&mut self, entered: ObjId) {
        let watchers: Vec<ObjId> = self
            .state
            .players
            .iter()
            .flat_map(|p| p.battlefield.iter().copied())
            .collect();
        for watcher in watchers {
            let wctrl = self.state.object(watcher).controller;
            // Clone the (index, filter) of this watcher's PermanentEnters triggers so the `def`
            // borrow is released before we mutate `pending_triggers`.
            let candidates: Vec<(u32, CardFilter)> = match self.state.def_of(watcher) {
                Some(def) => def
                    .abilities
                    .iter()
                    .enumerate()
                    .filter_map(|(i, a)| match a {
                        Ability::Triggered { event: EventPattern::PermanentEnters(f), .. } => {
                            Some((i as u32, f.clone()))
                        }
                        _ => None,
                    })
                    .collect(),
                None => continue,
            };
            for (index, filter) in candidates {
                if self.enter_filter_matches(entered, &filter, wctrl) {
                    let id = self.state.mint_stack();
                    self.state.pending_triggers.push(StackObject {
                        id,
                        controller: wctrl,
                        source: Some(watcher),
                        kind: StackObjectKind::Ability { index },
                        targets: Vec::new(),
                        x: None,
                    });
                }
            }
        }
    }

    /// Whether the just-entered object matches a `PermanentEnters` filter, with `ControlledBy`
    /// resolved against the watching permanent's controller (`watcher_controller`).
    fn enter_filter_matches(&self, obj: ObjId, filter: &CardFilter, watcher_controller: PlayerId) -> bool {
        let cc = self.state.computed(obj);
        match filter {
            CardFilter::Any => true,
            CardFilter::HasCardType(t) => cc.card_types.contains(t),
            CardFilter::HasSubtype(s) => cc.subtypes.contains(s),
            CardFilter::HasColor(c) => cc.colors.contains(c),
            CardFilter::Colorless => cc.colors.is_empty(),
            CardFilter::ControlledBy(pref) => {
                let want = match pref {
                    PlayerRef::Opponent | PlayerRef::EachOpponent => self
                        .state
                        .players
                        .iter()
                        .map(|p| p.id)
                        .find(|&q| q != watcher_controller)
                        .unwrap_or(watcher_controller),
                    _ => watcher_controller, // Controller / Owner
                };
                self.state.objects.get(&obj).map(|o| o.controller) == Some(want)
            }
            CardFilter::All(fs) => fs.iter().all(|f| self.enter_filter_matches(obj, f, watcher_controller)),
            CardFilter::AnyOf(fs) => fs.iter().any(|f| self.enter_filter_matches(obj, f, watcher_controller)),
            CardFilter::Not(f) => !self.enter_filter_matches(obj, f, watcher_controller),
            _ => false,
        }
    }
}

/// Collect the `TargetSpec`s an `Effect` requires, in declaration order (CR 601.2c). The
/// milestone-3 starter set only needs the `DealDamage` target; `Sequence` recurses. Other
/// targeted IR nodes are added as their cards arrive.
fn collect_target_specs(effect: &Effect) -> Vec<TargetSpec> {
    let mut out = Vec::new();
    collect_specs_into(effect, &mut out);
    out
}

fn collect_specs_into(effect: &Effect, out: &mut Vec<TargetSpec>) {
    match effect {
        Effect::DealDamage {
            to: EffectTarget::Target(spec),
            ..
        }
        | Effect::Destroy {
            what: EffectTarget::Target(spec),
        }
        | Effect::Attach {
            to: EffectTarget::Target(spec),
            ..
        } => out.push(spec.clone()),
        Effect::Sequence(effects) => {
            for e in effects {
                collect_specs_into(e, out);
            }
        }
        _ => {}
    }
}

/// Turn a `ChooseTargets` response into the chosen concrete targets (in slot order). Defensive
/// against a malformed/empty response: falls back to the first legal candidate of each
/// required slot so a misbehaving agent can't produce an under-targeted spell.
fn parse_targets(slots: &[TargetSlot], resp: &DecisionResponse) -> Vec<Target> {
    let mut chosen = Vec::new();
    if let DecisionResponse::Pairs(pairs) = resp {
        for (slot_idx, cand_idx) in pairs {
            if let Some(slot) = slots.get(*slot_idx as usize) {
                if let Some(t) = slot.legal.get(*cand_idx as usize) {
                    chosen.push(*t);
                }
            }
        }
    }
    if chosen.is_empty() {
        for slot in slots {
            if slot.min > 0 {
                if let Some(t) = slot.legal.first() {
                    chosen.push(*t);
                }
            }
        }
    }
    chosen
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

    /// A scripted agent that mulligans a fixed number of times then keeps, bottoming the first
    /// `min` cards when asked (CR 103.5c). Passes on everything else.
    struct MulliganThenKeep {
        remaining: u32,
    }
    impl Agent for MulliganThenKeep {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Mulligan { .. } => {
                    if self.remaining > 0 {
                        self.remaining -= 1;
                        DecisionResponse::Bool(true)
                    } else {
                        DecisionResponse::Bool(false)
                    }
                }
                DecisionRequest::SelectCards { min, .. } => {
                    DecisionResponse::Indices((0..*min).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn london_mulligan_bottoms_one_card_per_mulligan() {
        let mut state = GameState::new(2, 5);
        for seat in 0..2u32 {
            for _ in 0..30 {
                state.add_card(PlayerId(seat), Characteristics::basic_land("Forest"), Zone::Library);
            }
        }
        let total = state.objects.len();
        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(MulliganThenKeep { remaining: 2 }), // seat 0 mulligans twice
            Box::new(MulliganThenKeep { remaining: 0 }), // seat 1 keeps its first hand
        ];
        let mut engine = Engine::new(state, agents);
        engine.start_game();

        // Seat 0: kept a fresh seven, then bottomed two (one per mulligan) → 5 in hand.
        assert_eq!(engine.state.player(PlayerId(0)).hand.len(), 5);
        // Seat 1: kept its opening hand untouched.
        assert_eq!(engine.state.player(PlayerId(1)).hand.len(), 7);
        // Conservation: every card is still somewhere (hand or library), none lost.
        let p0 = engine.state.player(PlayerId(0));
        let p1 = engine.state.player(PlayerId(1));
        assert_eq!(
            p0.hand.len() + p0.library.len() + p1.hand.len() + p1.library.len(),
            total
        );
        // The bottomed cards went to the bottom (front of the vec; `draw` pops the end).
        assert_eq!(engine.state.player(PlayerId(0)).library.len(), 25);
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
            x: None,
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

    #[test]
    fn demo_deck_self_play_runs_to_completion() {
        // The milestone-3 EXIT: a real game (lands → creatures → attack → damage → 0 life,
        // or decking) plays to completion with RandomAgents, no panics, cards conserved.
        for seed in 0..40u64 {
            let state = crate::cards::two_player_demo_game(seed);
            let total = state.objects.len();
            let agents: Vec<Box<dyn Agent>> = vec![
                Box::new(RandomAgent::new(seed ^ 0xA11CE)),
                Box::new(RandomAgent::new(seed ^ 0xB0B)),
            ];
            let mut engine = Engine::new(state, agents);
            engine.run_game();
            assert!(engine.state.game_over, "game must end (seed {seed})");
            // No tokens/copies in the starter set ⇒ object count is conserved.
            assert_eq!(engine.state.objects.len(), total, "card conservation (seed {seed})");
            assert!(
                engine.state.living_players().len() <= 1,
                "≤1 survivor (seed {seed})"
            );
        }
    }

    #[test]
    fn burn_vs_bears_self_play_completes() {
        // The user's hand-test matchup, under RandomAgents: must terminate, no panic, cards
        // conserved. (Burn = 40 Bolt + 20 Mountain vs Bears = 40 Grizzly Bears + 20 Forest.)
        for seed in 0..20u64 {
            let state = crate::cards::burn_vs_bears_game(seed);
            let total = state.objects.len();
            let agents: Vec<Box<dyn Agent>> = vec![
                Box::new(RandomAgent::new(seed ^ 0xB)),
                Box::new(RandomAgent::new(seed ^ 0xE)),
            ];
            let mut engine = Engine::new(state, agents);
            engine.run_game();
            assert!(engine.state.game_over, "seed {seed}");
            assert_eq!(engine.state.objects.len(), total, "card conservation (seed {seed})");
        }
    }

    #[test]
    fn skip_opening_deal_leaves_a_built_scenario_untouched() {
        // The scenario hook (webui): no shuffle, no opening draw, so an exact hand/board can
        // be placed without decking out.
        let state = crate::cards::two_player_demo_game(1);
        let mut engine = Engine::new(
            state,
            vec![Box::new(RandomAgent::new(1)), Box::new(RandomAgent::new(2))],
        );
        engine.skip_opening_deal();
        engine.start_game(); // no-op now
        assert!(engine.state.player(PlayerId(0)).hand.is_empty(), "no opening deal");
        assert_eq!(
            engine.state.player(PlayerId(0)).library.len(),
            30,
            "library untouched (no shuffle/draw)"
        );
    }

    #[test]
    fn outcome_reports_winner_and_reason() {
        let mut engine = lands_only_game(12, 5);
        engine.start_game();
        engine.state.player_mut(PlayerId(1)).life = 0;
        engine.run_agenda();
        let outcome = engine.outcome();
        assert_eq!(outcome.winner, Some(PlayerId(0)));
        assert_eq!(outcome.reason, super::EndReason::ZeroLife);
    }
}

/// Inline snapshot ("expect") tests for milestone-2 behaviour: the enumerated legal options
/// at a decision point (masking is the engine's job) and the CR-500s turn-structure trace.
#[cfg(test)]
mod expect_tests {
    use super::*;
    use crate::agent::{DecisionResponse, PlayerView, RandomAgent};
    use crate::basics::{Phase, Target, Zone};
    use crate::cards::{self, grp};
    use crate::ids::PlayerId;
    use crate::state::{Characteristics, GameState};
    use expect_test::expect;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Records the size of each `ChooseReplacement` request it answers (always picks index 0),
    /// so a test can assert the CR 616.1f decision was surfaced; otherwise passes/declines.
    struct ReplacementSpy {
        seen: Rc<RefCell<Vec<usize>>>,
    }
    impl Agent for ReplacementSpy {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseReplacement { applicable, .. } => {
                    self.seen.borrow_mut().push(applicable.len());
                    DecisionResponse::Index(0)
                }
                DecisionRequest::Priority { actions, .. } => {
                    // Cast the first castable (to get the creature down), else pass.
                    match actions.iter().position(|a| matches!(a, PlayableAction::Cast { .. })) {
                        Some(i) => DecisionResponse::Action(i as u32),
                        None => DecisionResponse::Pass,
                    }
                }
                DecisionRequest::SelectCards { min, .. } => {
                    DecisionResponse::Indices((0..*min).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

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

    /// Activates the first available activated ability (e.g. Equip), choosing target slot 0;
    /// otherwise passes. For the equipment test.
    struct EquipAgent;
    impl Agent for EquipAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Priority { actions, .. } => actions
                    .iter()
                    .position(|a| matches!(a, PlayableAction::Activate { .. }))
                    .map(|i| DecisionResponse::Action(i as u32))
                    .unwrap_or(DecisionResponse::Pass),
                DecisionRequest::ChooseTargets { slots, .. } => DecisionResponse::Pairs(
                    slots.iter().enumerate().map(|(si, _)| (si as u32, 0u32)).collect(),
                ),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Activates the activated ability with a specific index (e.g. a chosen loyalty ability),
    /// choosing target slot 0; otherwise passes. For the planeswalker tests.
    struct ActivateAgent {
        want: u32,
    }
    impl Agent for ActivateAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Priority { actions, .. } => actions
                    .iter()
                    .position(|a| matches!(a, PlayableAction::Activate { ability, .. } if ability.0 == self.want))
                    .map(|i| DecisionResponse::Action(i as u32))
                    .unwrap_or(DecisionResponse::Pass),
                DecisionRequest::ChooseTargets { slots, .. } => DecisionResponse::Pairs(
                    slots.iter().enumerate().map(|(si, _)| (si as u32, 0u32)).collect(),
                ),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Casts the first castable spell, chooses a fixed X for `ChooseNumber`, and targets an
    /// opponent player. For the X-cost test.
    struct XCastAgent(i64);
    impl Agent for XCastAgent {
        fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Priority { actions, .. } => actions
                    .iter()
                    .position(|a| matches!(a, PlayableAction::Cast { .. }))
                    .map(|i| DecisionResponse::Action(i as u32))
                    .unwrap_or(DecisionResponse::Pass),
                DecisionRequest::ChooseNumber { .. } => DecisionResponse::Number(self.0),
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let me = view.seat;
                    let pairs = slots
                        .iter()
                        .enumerate()
                        .map(|(si, slot)| {
                            let idx = slot
                                .legal
                                .iter()
                                .position(|t| matches!(t, Target::Player(p) if *p != me))
                                .unwrap_or(0);
                            (si as u32, idx as u32)
                        })
                        .collect();
                    DecisionResponse::Pairs(pairs)
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// Answers `ChooseModes` by picking a fixed mode index; otherwise passes. For the modal test.
    struct ModeAgent(u32);
    impl Agent for ModeAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::ChooseModes { .. } => DecisionResponse::Indices(vec![self.0]),
                _ => DecisionResponse::Pass,
            }
        }
    }

    /// A deterministic, aggressive agent for casting/combat tests: at priority it casts the
    /// first castable spell, else plays the first land, else passes; it attacks with
    /// everything; never blocks; targets an opponent (player) when choosing a target.
    struct AggroAgent;
    impl Agent for AggroAgent {
        fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Priority { actions, .. } => {
                    if let Some(i) = actions
                        .iter()
                        .position(|a| matches!(a, PlayableAction::Cast { .. }))
                    {
                        return DecisionResponse::Action(i as u32);
                    }
                    if let Some(i) = actions
                        .iter()
                        .position(|a| matches!(a, PlayableAction::PlayLand { .. }))
                    {
                        return DecisionResponse::Action(i as u32);
                    }
                    DecisionResponse::Pass
                }
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let me = view.seat;
                    let mut pairs = Vec::new();
                    for (si, slot) in slots.iter().enumerate() {
                        let idx = slot
                            .legal
                            .iter()
                            .position(|t| matches!(t, Target::Player(p) if *p != me))
                            .or_else(|| {
                                slot.legal.iter().position(|t| matches!(t, Target::Player(_)))
                            })
                            .unwrap_or(0);
                        pairs.push((si as u32, idx as u32));
                    }
                    DecisionResponse::Pairs(pairs)
                }
                DecisionRequest::DeclareAttackers { eligible } => {
                    let pairs = eligible
                        .iter()
                        .enumerate()
                        .filter(|(_, o)| !o.may_attack.is_empty())
                        .map(|(i, _)| (i as u32, 0u32))
                        .collect();
                    DecisionResponse::Pairs(pairs)
                }
                DecisionRequest::AssignCombatDamage { total, .. } => {
                    DecisionResponse::Amounts(vec![(0, *total)])
                }
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

    /// Records the phase of every `Priority` window it is actually prompted at (so a test can
    /// assert which windows the Arena auto-pass policy elided). Always passes.
    struct PrioritySpy {
        prompted: Rc<RefCell<Vec<Phase>>>,
    }
    impl Agent for PrioritySpy {
        fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            if let DecisionRequest::Priority { .. } = req {
                self.prompted.borrow_mut().push(view.phase);
            }
            match req {
                DecisionRequest::SelectCards { min, .. } => {
                    DecisionResponse::Indices((0..*min).collect())
                }
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn auto_pass_policy_follows_arena_rules() {
        // Direct unit tests of the policy (should_auto_pass), P0 active.
        let state = cards::build_game(1, &[&[], &[]]);
        let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
        e.set_arena_auto_pass(true);
        e.state.active_player = PlayerId(0);
        let (p0, p1) = (PlayerId(0), PlayerId(1));

        let set_phase = |e: &mut Engine, ph| e.state.phase = ph;

        // Own main phases are persistent default stops — prompt even with no action.
        set_phase(&mut e, Phase::PrecombatMain);
        assert!(!e.should_auto_pass(p0, false), "own MP1 is a stop");
        set_phase(&mut e, Phase::PostcombatMain);
        assert!(!e.should_auto_pass(p0, false), "own MP2 is a stop");

        // No action at a non-stop step ⇒ auto-pass.
        set_phase(&mut e, Phase::Upkeep);
        assert!(e.should_auto_pass(p0, false), "no action ⇒ auto-pass upkeep");
        // SmartStops (default ON): a legal play at ANY non-stop step ⇒ prompt (even upkeep).
        assert!(!e.should_auto_pass(p0, true), "SmartStops prompts where you can act");
        set_phase(&mut e, Phase::CombatDamage);
        assert!(!e.should_auto_pass(p0, true), "SmartStops prompts in combat damage too");

        // Declare-attackers/blockers are NOT persistent default stops (forced TBAs); with no
        // action they auto-pass.
        set_phase(&mut e, Phase::DeclareAttackers);
        assert!(e.should_auto_pass(p0, false), "declare-attackers not a default priority stop");
        set_phase(&mut e, Phase::DeclareBlockers);
        assert!(e.should_auto_pass(p1, false), "declare-blockers not a default priority stop");

        // SmartStops OFF means "only my explicit stops": auto-pass EVERY non-stop empty-stack
        // window even with an action — including combat damage.
        e.set_smart_stops(p0, false);
        set_phase(&mut e, Phase::Upkeep);
        assert!(e.should_auto_pass(p0, true), "SmartStops off: pass upkeep with an action");
        set_phase(&mut e, Phase::CombatDamage);
        assert!(e.should_auto_pass(p0, true), "SmartStops off: pass combat damage with an action");
        // …but an actual stop (own main phase) still prompts.
        set_phase(&mut e, Phase::PrecombatMain);
        assert!(!e.should_auto_pass(p0, true), "SmartStops off: own MP1 is still a stop");
        e.set_smart_stops(p0, true);

        // A manual stop forces a prompt at an otherwise-elided step.
        e.set_stop(p0, Phase::Upkeep, Some(true));
        set_phase(&mut e, Phase::Upkeep);
        assert!(!e.should_auto_pass(p0, false), "manual upkeep stop");

        // Full control stops everywhere.
        e.set_full_control(p1, true);
        set_phase(&mut e, Phase::End);
        assert!(!e.should_auto_pass(p1, false), "full control stops at the end step");

        // With the policy off (paper-CR), nothing is ever auto-passed.
        e.set_arena_auto_pass(false);
        assert!(!e.should_auto_pass(p0, false));
    }

    #[test]
    fn auto_pass_elides_minor_steps_over_a_turn() {
        // End-to-end: P0 has no actions all turn; with the Arena profile on, P0 is prompted
        // only at its default stops (MP1, declare-attackers, MP2), never at upkeep/draw/etc.
        let prompted = Rc::new(RefCell::new(Vec::new()));
        let state = cards::build_game(1, &[&[], &[]]); // empty libraries
        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(PrioritySpy { prompted: Rc::clone(&prompted) }),
            Box::new(PassAgent),
        ];
        let mut e = Engine::new(state, agents);
        e.skip_opening_deal(); // empty hands; no draw on turn 1 (P0 on the play)
        e.set_arena_auto_pass(true);
        e.start_game();
        e.take_turn(); // P0's whole turn

        let seen = prompted.borrow().clone();
        for elided in [
            Phase::Upkeep,
            Phase::Draw,
            Phase::BeginCombat,
            Phase::CombatDamage,
            Phase::EndCombat,
            Phase::End,
        ] {
            assert!(!seen.contains(&elided), "{elided:?} should be auto-passed");
        }
        assert!(seen.contains(&Phase::PrecombatMain), "stop at own MP1");
        assert!(seen.contains(&Phase::PostcombatMain), "stop at own MP2");

        // Full control prompts at the minor steps too.
        let prompted2 = Rc::new(RefCell::new(Vec::new()));
        let state2 = cards::build_game(1, &[&[], &[]]);
        let agents2: Vec<Box<dyn Agent>> = vec![
            Box::new(PrioritySpy { prompted: Rc::clone(&prompted2) }),
            Box::new(PassAgent),
        ];
        let mut e2 = Engine::new(state2, agents2);
        e2.skip_opening_deal();
        e2.set_arena_auto_pass(true);
        e2.set_full_control(PlayerId(0), true);
        e2.start_game();
        e2.take_turn();
        let seen2 = prompted2.borrow().clone();
        assert!(seen2.contains(&Phase::Upkeep), "full control stops at upkeep");
        assert!(seen2.contains(&Phase::BeginCombat), "full control stops at begin combat");
    }

    #[test]
    fn player_view_carries_stop_state_for_the_ui() {
        // PlayerView.stops echoes the seat's Arena settings (render-only): None when the
        // profile is off, populated with full_control/smart_stops/resolve_own_stack + the
        // effective per-step stops when on.
        let state = cards::build_game(1, &[&[], &[]]);
        let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
        e.state.active_player = PlayerId(0);

        // Off by default → no stop state echoed.
        assert!(e.view_for_seat(PlayerId(0)).stops.is_none());

        e.set_arena_auto_pass(true);
        e.set_stop(PlayerId(0), Phase::Upkeep, Some(true)); // manual extra stop
        let view = e.view_for_seat(PlayerId(0));
        let s = view.stops.expect("stops echoed when the profile is on");
        assert!(s.smart_stops && s.resolve_own_stack && !s.full_control, "MTGA defaults");
        let stop_at = |ph: Phase| s.per_step.iter().find(|(p, _)| *p == ph).map(|(_, b)| *b);
        assert_eq!(stop_at(Phase::PrecombatMain), Some(true), "own MP1 default stop");
        assert_eq!(stop_at(Phase::PostcombatMain), Some(true), "own MP2 default stop");
        assert_eq!(stop_at(Phase::Upkeep), Some(true), "manual upkeep stop reflected");
        assert_eq!(stop_at(Phase::Draw), Some(false), "draw not a stop");
        // Untap/Cleanup grant no priority → not listed.
        assert!(stop_at(Phase::Untap).is_none());
    }

    #[test]
    fn stack_auto_pass_resolves_your_own_objects() {
        // stackAutoPassOption = ResolveMyStackEffects (MTGA default): while your own object is
        // on top of the stack you auto-pass (let it resolve, don't respond to yourself); the
        // opponent is still prompted to respond when they can act.
        use crate::stack::{StackObject, StackObjectKind};
        let state = cards::build_game(1, &[&[], &[]]);
        let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
        e.set_arena_auto_pass(true);
        e.state.active_player = PlayerId(0);
        e.state.phase = Phase::PrecombatMain; // a stop step — but the stack policy overrides
        let sid = e.state.mint_stack();
        e.state.stack.push(StackObject {
            id: sid,
            controller: PlayerId(0),
            source: None,
            kind: StackObjectKind::Ability { index: 0 },
            targets: vec![],
            x: None,
        });

        // P0's own object on top → auto-pass (even in MP1, even with an action available).
        assert!(e.should_auto_pass(PlayerId(0), true), "ResolveMyStackEffects auto-passes own object");
        // The opponent is prompted to respond iff they can act.
        assert!(!e.should_auto_pass(PlayerId(1), true), "opponent prompted to respond (has play)");
        assert!(e.should_auto_pass(PlayerId(1), false), "opponent auto-passes with no response");
        // resolve_own_stack OFF ⇒ P0 may respond to its own object.
        e.set_resolve_own_stack(PlayerId(0), false);
        assert!(!e.should_auto_pass(PlayerId(0), true), "respond-to-self when resolve_own_stack off");
        // Full control stops over the stack regardless.
        e.set_full_control(PlayerId(1), true);
        assert!(!e.should_auto_pass(PlayerId(1), false), "full control stops over the stack");
    }

    #[test]
    fn stops_handle_toggles_stops_live() {
        // A UI session holds the seat's StopConfig handle and mutates it mid-game (from the
        // socket thread); the engine re-reads the shared config at the next window with no reset.
        let state = cards::build_game(1, &[&[], &[]]);
        let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
        e.set_arena_auto_pass(true);
        e.state.active_player = PlayerId(0);

        // Upkeep is not a default stop.
        assert!(!e.is_stop(PlayerId(0), Phase::Upkeep));
        let handle = e.stops_handle(PlayerId(0));
        // Mutate through the handle (what the socket task does on a `SetStop`): stop on MY upkeep
        // only (own_turn = true).
        handle.lock().unwrap().set_override(Phase::Upkeep, true, Some(true));
        assert!(e.is_stop(PlayerId(0), Phase::Upkeep), "engine sees the live toggle (own turn)");
        // The override is per side: on the OPPONENT's turn this seat still passes upkeep.
        e.state.active_player = PlayerId(1);
        assert!(!e.is_stop(PlayerId(0), Phase::Upkeep), "opponent-turn upkeep is unaffected");
        e.state.active_player = PlayerId(0);
        // Revert the own-turn override.
        handle.lock().unwrap().set_override(Phase::Upkeep, true, None);
        assert!(!e.is_stop(PlayerId(0), Phase::Upkeep), "revert restores the Arena default");

        // The handle aliases the engine's own config (same Arc), and each seat is independent.
        assert!(!e.stops_handle(PlayerId(1)).lock().unwrap().full_control);
        e.set_full_control(PlayerId(0), true);
        assert!(handle.lock().unwrap().full_control, "engine setter is visible through the handle");
        assert!(!e.stops_handle(PlayerId(1)).lock().unwrap().full_control, "seats are independent");

        // effective_steps (the display echo): own-turn shows MP1/MP2 as the persistent defaults;
        // the opponent-turn side defaults to no stops.
        let cfg = e.stop_config(PlayerId(1));
        let own = cfg.effective_steps(true);
        let onside = |eff: &[(Phase, bool)], ph: Phase| eff.iter().find(|(p, _)| *p == ph).map(|(_, b)| *b);
        assert_eq!(onside(&own, Phase::PrecombatMain), Some(true));
        assert_eq!(onside(&own, Phase::PostcombatMain), Some(true));
        assert_eq!(onside(&own, Phase::Upkeep), Some(false));
        let opp = cfg.effective_steps(false);
        assert_eq!(onside(&opp, Phase::PrecombatMain), Some(false), "no default stops on the opponent's turn");
    }

    #[test]
    fn aura_enters_attached_and_buffs_its_host() {
        // Cast Rancor on a creature: the Aura targets at cast (601.2c), enters the battlefield
        // attached (608.3e), and its AttachedHost statics buff the host (+2/+0, trample).
        let mut state = synth_state(1);
        let bears = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield); // pay {G}
        let rancor = put(&mut state, PlayerId(0), synth::TRAMPLE_AURA, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);
        e.priority_round();

        assert_eq!(e.state.object(rancor).zone, Zone::Battlefield, "Rancor resolved onto the battlefield");
        assert_eq!(
            e.state.object(rancor).attached_to,
            Some(bears),
            "Rancor entered attached to the chosen creature"
        );
        let cc = e.state.computed(bears);
        assert_eq!(cc.power, Some(4), "enchanted creature is +2/+0");
        assert!(cc.has_keyword(Keyword::Trample), "and has trample");
    }

    #[test]
    fn aura_falls_off_into_graveyard_when_its_host_leaves() {
        // CR 704.5 (Auras): once the host leaves the battlefield the Aura is no longer attached
        // to a legal object, so a state-based action puts it into its owner's graveyard.
        let mut state = synth_state(1);
        let bears = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let rancor = put(&mut state, PlayerId(0), synth::TRAMPLE_AURA, Zone::Battlefield);
        state.objects.get_mut(&rancor).unwrap().attached_to = Some(bears);
        state.mark_chars_dirty();
        let mut e = pass_engine(state);

        // Attached to a creature → no fall-off.
        assert!(
            !sba::collect(&e.state)
                .iter()
                .any(|s| matches!(s, StateBasedAction::AuraFallsOff { .. })),
            "no fall-off while legally attached"
        );
        // Host leaves (e.g. dies): `move_object` unattaches the Aura, then the SBA collects it.
        e.state.move_object(bears, Zone::Graveyard, PlayerId(0));
        let sbas = sba::collect(&e.state);
        assert!(
            sbas.iter()
                .any(|s| matches!(s, StateBasedAction::AuraFallsOff { aura } if *aura == rancor)),
            "the now-unattached Aura is collected as a fall-off SBA"
        );
        e.perform_sbas(&sbas);
        assert_eq!(e.state.object(rancor).zone, Zone::Graveyard, "Aura fell off into the graveyard");
    }

    #[test]
    fn equipment_equips_a_creature_then_unattaches_when_it_dies() {
        // Equip is a sorcery-speed activated ability: pay {1}, attach Bonesplitter to a creature
        // you control, and its AttachedHost static gives +2/+0. When the host dies the Equipment
        // stays on the battlefield, just unattached (CR 704.5q).
        let mut state = cards::build_game(1, &[&[], &[]]);
        let bears = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        let saw = put(&mut state, PlayerId(0), grp::BONESPLITTER, Zone::Battlefield);
        put(&mut state, PlayerId(0), grp::MOUNTAIN, Zone::Battlefield); // pay Equip {1}
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(EquipAgent), Box::new(PassAgent)]);
        e.priority_round();

        assert_eq!(e.state.object(saw).attached_to, Some(bears), "Bonesplitter equipped the creature");
        assert_eq!(e.state.computed(bears).power, Some(4), "equipped creature is +2/+0");
        assert_eq!(e.state.computed(bears).toughness, Some(2));

        // The creature dies → Equipment unattaches but remains on the battlefield.
        e.state.move_object(bears, Zone::Graveyard, PlayerId(0));
        assert_eq!(e.state.object(saw).zone, Zone::Battlefield, "Equipment stays on the battlefield");
        assert_eq!(e.state.object(saw).attached_to, None, "but is no longer attached");
    }

    #[test]
    fn planeswalker_enters_with_loyalty_and_dies_at_zero() {
        // CR 306.5b: a planeswalker enters with loyalty counters equal to printed loyalty.
        // CR 704.5i: a planeswalker with 0 loyalty is put into its owner's graveyard.
        use crate::basics::{CardType, CounterKind};
        let mut state = cards::build_game(1, &[&[], &[]]);
        let chars = Characteristics {
            name: "Test Walker".into(),
            card_types: vec![CardType::Planeswalker],
            loyalty: Some(3),
            ..Default::default()
        };
        let pw = state.add_card(PlayerId(0), chars, Zone::Battlefield);
        assert_eq!(
            state.object(pw).counters.get(&CounterKind::Loyalty),
            3,
            "entered with printed loyalty"
        );
        let mut e = pass_engine(state);
        assert!(
            !sba::collect(&e.state)
                .iter()
                .any(|s| matches!(s, StateBasedAction::PlaneswalkerDies { .. })),
            "alive while loyalty > 0"
        );
        // Loyalty drained to 0 → 704.5i.
        e.state.objects.get_mut(&pw).unwrap().counters.counts.insert(CounterKind::Loyalty, 0);
        let sbas = sba::collect(&e.state);
        assert!(
            sbas.iter()
                .any(|s| matches!(s, StateBasedAction::PlaneswalkerDies { pw: x } if *x == pw)),
            "0-loyalty planeswalker is collected as an SBA"
        );
        e.perform_sbas(&sbas);
        assert_eq!(e.state.object(pw).zone, Zone::Graveyard, "0-loyalty planeswalker dies");
    }

    #[test]
    fn loyalty_plus_ability_adds_loyalty_and_resolves() {
        use crate::basics::CounterKind;
        let mut state = synth_state(1);
        let chandra = put(&mut state, PlayerId(0), synth::WALKER, Zone::Battlefield); // loyalty 5
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(ActivateAgent { want: 0 }), Box::new(PassAgent)]);
        e.priority_round();
        assert_eq!(
            e.state.object(chandra).counters.get(&CounterKind::Loyalty),
            7,
            "+2 loyalty (5 → 7)"
        );
        assert_eq!(e.state.player(PlayerId(1)).life, 18, "+2 dealt 2 to the opponent");
    }

    #[test]
    fn loyalty_minus_ability_pays_loyalty_and_deals_damage() {
        use crate::basics::CounterKind;
        let mut state = synth_state(1);
        let chandra = put(&mut state, PlayerId(0), synth::WALKER, Zone::Battlefield); // loyalty 5
        let prey = put(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(ActivateAgent { want: 1 }), Box::new(PassAgent)]);
        e.priority_round();
        assert_eq!(
            e.state.object(chandra).counters.get(&CounterKind::Loyalty),
            2,
            "−3 loyalty (5 → 2)"
        );
        assert_eq!(e.state.object(prey).zone, Zone::Graveyard, "−3 dealt 4 to the 2/2, killing it");
    }

    #[test]
    fn loyalty_ability_is_once_per_turn_across_all_abilities() {
        // CR 606.3: the limit is PER PLANESWALKER across ALL its loyalty abilities — using +2
        // also locks out −3 this turn (not just a second +2). A creature is on the board so −3
        // would otherwise be legal (loyalty 7 ≥ 3, a target exists) — proving the flag blocks the
        // OTHER ability, not merely re-use of the same one.
        let mut state = synth_state(1);
        put(&mut state, PlayerId(0), synth::WALKER, Zone::Battlefield);
        put(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield); // a legal −3 target
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(ActivateAgent { want: 0 }), Box::new(PassAgent)]);
        e.priority_round();
        assert!(
            !e.legal_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::Activate { .. })),
            "after using +2, NO loyalty ability (incl. the otherwise-legal −3) is available this turn"
        );
    }

    #[test]
    fn cannot_activate_a_minus_ability_without_enough_loyalty() {
        use crate::basics::CounterKind;
        let mut state = synth_state(1);
        let chandra = put(&mut state, PlayerId(0), synth::WALKER, Zone::Battlefield);
        put(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield); // a legal −3 target
        state.objects.get_mut(&chandra).unwrap().counters.counts.insert(CounterKind::Loyalty, 2);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
        let abilities: Vec<u32> = e
            .legal_actions(PlayerId(0))
            .iter()
            .filter_map(|a| match a {
                PlayableAction::Activate { ability, .. } => Some(ability.0),
                _ => None,
            })
            .collect();
        assert!(abilities.contains(&0), "+2 is always payable");
        assert!(!abilities.contains(&1), "−3 needs ≥3 loyalty (has 2)");
    }

    #[test]
    fn put_counters_adds_a_plus_one_counter_c2() {
        use crate::basics::CounterKind;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::value::ValueExpr;
        use crate::effects::{Effect, EffectTarget};
        let mut state = cards::build_game(1, &[&[], &[]]);
        let bears = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        let mut e = pass_engine(state);
        e.resolve_effect(
            &Effect::PutCounters {
                what: EffectTarget::SourceSelf,
                kind: CounterKind::PlusOnePlusOne,
                n: ValueExpr::Fixed(1),
            },
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(bears), ..Default::default() },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        assert_eq!(e.state.object(bears).counters.get(&CounterKind::PlusOnePlusOne), 1);
        assert_eq!(e.state.computed(bears).power, Some(3), "+1/+1 counter boosts computed P/T");
    }

    #[test]
    fn mill_moves_top_cards_to_graveyard_c3() {
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::Effect;
        let mut state = cards::build_game(1, &[&[grp::GRIZZLY_BEARS, grp::FOREST], &[]]);
        assert_eq!(state.player(PlayerId(0)).library.len(), 2);
        let mut e = pass_engine(state);
        e.resolve_effect(
            &Effect::Mill { who: PlayerRef::Controller, count: ValueExpr::Fixed(2) },
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).library.len(), 0, "milled both cards");
        assert_eq!(e.state.player(PlayerId(0)).graveyard.len(), 2, "into the graveyard");
    }

    #[test]
    fn create_token_puts_tokens_on_the_battlefield_c6() {
        use crate::basics::Color;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::target::TokenSpec;
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::Effect;
        let mut state = cards::build_game(1, &[&[], &[]]);
        let before = state.player(PlayerId(0)).battlefield.len();
        let mut e = pass_engine(state);
        e.resolve_effect(
            &Effect::CreateToken {
                spec: TokenSpec {
                    name: "Bird".into(),
                    card_types: vec![CardType::Creature],
                    subtypes: vec!["Bird".into()],
                    colors: vec![Color::White],
                    power: 1,
                    toughness: 1,
                    keywords: Vec::new(),
                    counters: Vec::new(),
                },
                count: ValueExpr::Fixed(2),
                controller: PlayerRef::Controller,
            },
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).battlefield.len(), before + 2, "two 1/1 Bird tokens");
    }

    #[test]
    fn value_count_counts_lands_you_control_c9() {
        use crate::basics::DamageKind;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::target::CardFilter;
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::{Effect, EffectTarget};
        let mut state = cards::build_game(1, &[&[], &[]]);
        put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield);
        put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield);
        put(&mut state, PlayerId(0), grp::MOUNTAIN, Zone::Battlefield);
        put(&mut state, PlayerId(1), grp::FOREST, Zone::Battlefield); // opponent's — doesn't count
        let mut e = pass_engine(state);
        e.resolve_effect(
            &Effect::DealDamage {
                amount: ValueExpr::Count {
                    zone: Zone::Battlefield,
                    filter: CardFilter::HasCardType(CardType::Land),
                    controller: Some(PlayerRef::Controller),
                },
                to: EffectTarget::Player(PlayerRef::Opponent),
                kind: DamageKind::Noncombat,
            },
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(1)).life, 17, "3 lands you control → 3 damage");
    }

    #[test]
    fn modal_resolves_only_the_chosen_mode_c7() {
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::{Effect, Mode};
        // "Choose one — gain 3 life; or draw a card." The agent picks mode 0.
        let modal = Effect::Modal {
            modes: vec![
                Mode {
                    label: "Gain 3 life".into(),
                    effect: Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(3) },
                },
                Mode {
                    label: "Draw a card".into(),
                    effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
                },
            ],
            min: 1,
            max: 1,
            allow_repeat: false,
        };
        let state = cards::build_game(1, &[&[grp::GRIZZLY_BEARS], &[]]); // P0 library has 1 card
        let mut e = Engine::new(state, vec![Box::new(ModeAgent(0)), Box::new(PassAgent)]);
        e.resolve_effect(
            &modal,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        assert_eq!(e.state.player(PlayerId(0)).life, 23, "chose 'gain 3 life'");
        assert_eq!(e.state.player(PlayerId(0)).library.len(), 1, "did not draw (the other mode)");
    }

    #[test]
    fn search_fetches_a_basic_land_tapped_c5() {
        use crate::basics::{CardType, ZonePos};
        use crate::basics::ZoneDest;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::target::CardFilter;
        use crate::effects::value::PlayerRef;
        use crate::effects::Effect;
        // P0 library: a Forest (basic land) + a Grizzly Bears. Search for a basic land → battlefield tapped.
        let state = cards::build_game(1, &[&[grp::FOREST, grp::GRIZZLY_BEARS], &[]]);
        let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]); // PassAgent auto-picks min cards
        e.resolve_effect(
            &Effect::Search {
                who: PlayerRef::Controller,
                zone: Zone::Library,
                filter: CardFilter::All(vec![
                    CardFilter::HasCardType(CardType::Land),
                    CardFilter::Supertype("Basic".into()),
                ]),
                min: 1,
                max: 1,
                to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
                tapped: true,
            },
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        let forest = e
            .state
            .player(PlayerId(0))
            .battlefield
            .iter()
            .copied()
            .find(|&id| e.state.object(id).chars.grp_id == grp::FOREST);
        assert!(forest.is_some(), "fetched the basic land onto the battlefield");
        assert!(e.state.object(forest.unwrap()).status.tapped, "the fetched land entered tapped");
        assert_eq!(e.state.player(PlayerId(0)).library.len(), 1, "only the non-basic remains in library");
    }

    #[test]
    fn fight_deals_mutual_damage_c8() {
        use crate::basics::Target;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::{Effect, EffectTarget};
        // A 3/3 Hill Giant fights a 2/2 Grizzly Bears: the bears dies (3 ≥ 2), the giant survives
        // with 2 marked damage.
        let mut state = cards::build_game(1, &[&[], &[]]);
        let giant = put(&mut state, PlayerId(0), grp::HILL_GIANT, Zone::Battlefield); // 3/3
        let bears = put(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        let mut e = pass_engine(state);
        e.resolve_effect(
            &Effect::Fight { a: EffectTarget::ChosenIndex(0), b: EffectTarget::ChosenIndex(1) },
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(giant), Target::Object(bears)],
                ..Default::default()
            },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        assert_eq!(e.state.object(giant).damage_marked, 2, "giant took 2 from the bears");
        assert_eq!(e.state.object(bears).damage_marked, 3, "bears took 3 from the giant (lethal)");
        // The lethal-damage SBA then kills the bears on the next agenda pass.
        assert!(
            crate::sba::collect(&e.state)
                .iter()
                .any(|s| matches!(s, crate::sba::StateBasedAction::CreatureDies { creature, .. } if *creature == bears)),
            "the 2/2 is marked for death"
        );
    }

    #[test]
    fn x_cost_chooses_pays_and_flows_to_resolution_c10() {
        use crate::basics::{CardType, Color, DamageKind, ManaCost};
        use crate::effects::ability::Ability;
        use crate::effects::target::{TargetKind, TargetSpec};
        use crate::effects::value::ValueExpr;
        use crate::effects::{Effect, EffectTarget};
        use std::collections::BTreeMap;
        use std::sync::Arc;
        // A synthetic "{X}{R}: deal X damage to any target".
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "X Bolt".into(),
                card_types: vec![CardType::Instant],
                colors: vec![Color::Red],
                mana_cost: Some(ManaCost {
                    generic: 0,
                    colored: BTreeMap::from([(Color::Red, 1)]),
                    x: 1,
                }),
                grp_id: 9300,
                ..Default::default()
            },
            abilities: vec![Ability::Spell {
                effect: Effect::DealDamage {
                    amount: ValueExpr::X,
                    to: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Any,
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                    kind: DamageKind::Noncombat,
                },
            }],
            mana_colors: Vec::new(),
            text: String::new(),
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let bolt = {
            let c = state.card_db().get(9300).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let _ = bolt;
        for _ in 0..3 {
            let m = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
            state.add_card(PlayerId(0), m, Zone::Battlefield); // pay {2}{R}
        }
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(XCastAgent(2)), Box::new(PassAgent)]);
        e.priority_round();
        assert_eq!(e.state.player(PlayerId(1)).life, 18, "X=2 → 2 damage; cost {{2}}{{R}} = all 3 Mountains tapped");
        assert!(
            e.state.player(PlayerId(0)).battlefield.iter().all(|&id| e.state.object(id).status.tapped),
            "all 3 Mountains tapped to pay {{X=2}}{{R}}"
        );
    }

    #[test]
    fn add_mana_fills_the_pool_c19() {
        use crate::basics::Color;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::target::ManaSpec;
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::Effect;
        let state = cards::build_game(1, &[&[], &[]]);
        let mut e = pass_engine(state);
        e.resolve_effect(
            &Effect::AddMana {
                who: PlayerRef::Controller,
                mana: ManaSpec {
                    produces: vec![(Color::Green, ValueExpr::Fixed(2))],
                    any_color: None,
                },
            },
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        assert_eq!(
            e.state.player(PlayerId(0)).mana_pool.amounts.get(&Color::Green).copied().unwrap_or(0),
            2,
            "AddMana added {{G}}{{G}} to the pool"
        );
    }

    #[test]
    fn counters_on_self_doubles_plus_one_counters_c9b() {
        use crate::basics::CounterKind;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::value::ValueExpr;
        use crate::effects::{Effect, EffectTarget};
        // Mossborn Hydra's landfall: "double the number of +1/+1 counters on this creature" =
        // add as many more as it already has — `n: CountersOnSelf(+1/+1)`.
        let mut state = cards::build_game(1, &[&[], &[]]);
        let bears = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        state
            .objects
            .get_mut(&bears)
            .unwrap()
            .counters
            .counts
            .insert(CounterKind::PlusOnePlusOne, 2);
        let mut e = pass_engine(state);
        e.resolve_effect(
            &Effect::PutCounters {
                what: EffectTarget::SourceSelf,
                kind: CounterKind::PlusOnePlusOne,
                n: ValueExpr::CountersOnSelf(CounterKind::PlusOnePlusOne),
            },
            &ResolutionCtx { controller: Some(PlayerId(0)), source: Some(bears), ..Default::default() },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        assert_eq!(
            e.state.object(bears).counters.get(&CounterKind::PlusOnePlusOne),
            4,
            "doubled 2 → 4 counters"
        );
        assert_eq!(e.state.computed(bears).power, Some(6), "base 2/2 + 4 counters = 6/6");
    }

    #[test]
    fn landfall_triggers_when_a_land_you_control_enters_c4() {
        // C4: a "whenever a land you control enters" trigger (modeled as PermanentEnters of a land
        // you control) fires when you play a land — here, putting a +1/+1 counter on the source.
        use crate::basics::{CardType, Color, CounterKind};
        use crate::effects::ability::{Ability, EventPattern};
        use crate::effects::target::CardFilter;
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::{Effect, EffectTarget};
        use std::sync::Arc;
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Landfall Bird".into(),
                card_types: vec![CardType::Creature],
                subtypes: vec!["Bird".into()],
                colors: vec![Color::Green],
                power: Some(0),
                toughness: Some(1),
                grp_id: 9100,
                ..Default::default()
            },
            abilities: vec![Ability::Triggered {
                event: EventPattern::PermanentEnters(CardFilter::All(vec![
                    CardFilter::HasCardType(CardType::Land),
                    CardFilter::ControlledBy(PlayerRef::Controller),
                ])),
                condition: None,
                intervening_if: false,
                effect: Effect::PutCounters {
                    what: EffectTarget::SourceSelf,
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(1),
                },
            }],
            mana_colors: Vec::new(),
            text: String::new(),
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let bird_chars = state.card_db().get(9100).unwrap().chars.clone();
        let bird = state.add_card(PlayerId(0), bird_chars, Zone::Battlefield);
        let forest_chars = state.card_db().get(grp::FOREST).unwrap().chars.clone();
        state.add_card(PlayerId(0), forest_chars, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);
        e.priority_round(); // P0 plays the land → landfall trigger → counter

        assert_eq!(
            e.state.object(bird).counters.get(&CounterKind::PlusOnePlusOne),
            1,
            "playing a land you control fired landfall (a +1/+1 counter)"
        );
    }

    /// Put a card (by grp_id) directly into a player's zone, returning its id.
    fn put(state: &mut GameState, owner: PlayerId, grp_id: u32, zone: Zone) -> crate::ids::ObjId {
        let chars = state.card_db().get(grp_id).unwrap().chars.clone();
        state.add_card(owner, chars, zone)
    }

    /// grp ids for synthetic test-only cards (self-contained stand-ins so subsystem tests don't
    /// depend on any shippable card).
    mod synth {
        pub const WALKER: u32 = 9600; // planeswalker, loyalty 5, +2 / −3
        pub const FOG: u32 = 9700; // 0/2, prevents combat damage to itself
        pub const COUNTER_CREATURE: u32 = 9800; // 0/0, enters with a +1/+1 counter
        pub const TRAMPLE_AURA: u32 = 9500; // aura, +2/+0 & trample on its host
    }

    /// A 2-player `GameState` whose card DB also contains the synthetic test cards above.
    fn synth_state(seed: u64) -> GameState {
        use crate::basics::{CardType, Color, CounterKind, DamageKind};
        use crate::cards::CardDef;
        use crate::effects::ability::{
            Ability, ActionPattern, Cost, CostComponent, Restriction, Rewrite, StaticContribution,
            Timing,
        };
        use crate::effects::condition::Duration;
        use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::{Effect, EffectTarget};
        use crate::state::Characteristics;

        let mut db = cards::starter_db();

        // Loyalty planeswalker (Chandra stand-in): +2 deals 2 to each opponent; −3 deals 4 to a creature.
        db.insert(CardDef {
            chars: Characteristics {
                name: "Test Walker".into(),
                card_types: vec![CardType::Planeswalker],
                loyalty: Some(5),
                grp_id: synth::WALKER,
                ..Default::default()
            },
            abilities: vec![
                Ability::Activated {
                    cost: Cost { mana: None, components: vec![CostComponent::Loyalty(2)] },
                    effect: Effect::DealDamage {
                        amount: ValueExpr::Fixed(2),
                        to: EffectTarget::Player(PlayerRef::EachOpponent),
                        kind: DamageKind::Noncombat,
                    },
                    timing: Timing::Sorcery,
                    restriction: Some(Restriction::OncePerTurn),
                    is_mana: false,
                },
                Ability::Activated {
                    cost: Cost { mana: None, components: vec![CostComponent::Loyalty(-3)] },
                    effect: Effect::DealDamage {
                        amount: ValueExpr::Fixed(4),
                        to: EffectTarget::Target(TargetSpec {
                            kind: TargetKind::Creature(CardFilter::Any),
                            min: 1,
                            max: 1,
                            distinct: true,
                        }),
                        kind: DamageKind::Noncombat,
                    },
                    timing: Timing::Sorcery,
                    restriction: Some(Restriction::OncePerTurn),
                    is_mana: false,
                },
            ],
            mana_colors: Vec::new(),
            text: String::new(),
        });

        // 0/2 that prevents combat damage to itself (Fog Bank stand-in).
        db.insert(CardDef {
            chars: Characteristics {
                name: "Test Fog".into(),
                card_types: vec![CardType::Creature],
                subtypes: vec!["Wall".into()],
                colors: vec![Color::Blue],
                power: Some(0),
                toughness: Some(2),
                grp_id: synth::FOG,
                ..Default::default()
            },
            abilities: vec![Ability::Replacement {
                pattern: ActionPattern::WouldBeDealtDamage {
                    to: CardFilter::ItSelf,
                    kind: Some(DamageKind::Combat),
                },
                rewrite: Rewrite::Prevent,
            }],
            mana_colors: Vec::new(),
            text: String::new(),
        });

        // 0/0 that enters with a +1/+1 counter ({G}); drives the replacement-pass tests.
        db.insert(CardDef {
            chars: Characteristics {
                name: "Test Scaler".into(),
                card_types: vec![CardType::Creature],
                subtypes: vec!["Test".into()],
                colors: vec![Color::Green],
                mana_cost: Some(cards::mana_cost(0, &[(Color::Green, 1)])),
                power: Some(0),
                toughness: Some(0),
                grp_id: synth::COUNTER_CREATURE,
                ..Default::default()
            },
            abilities: vec![Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersWithCounters { kind: CounterKind::PlusOnePlusOne, n: 1 },
            }],
            mana_colors: Vec::new(),
            text: String::new(),
        });

        // Aura ({G}): +2/+0 & trample on the enchanted creature.
        let host = |c: StaticContribution| Ability::Static {
            contribution: c,
            affects: SelectSpec {
                zone: Zone::Battlefield,
                filter: CardFilter::AttachedHost,
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(0),
                max: ValueExpr::Fixed(0),
            },
            duration: Duration::WhileSourcePresent,
        };
        db.insert(CardDef {
            chars: Characteristics {
                name: "Test Aura".into(),
                card_types: vec![CardType::Enchantment],
                subtypes: vec!["Aura".into()],
                colors: vec![Color::Green],
                mana_cost: Some(cards::mana_cost(0, &[(Color::Green, 1)])),
                grp_id: synth::TRAMPLE_AURA,
                ..Default::default()
            },
            abilities: vec![
                host(StaticContribution::ModifyPT { power: 2, toughness: 0 }),
                host(StaticContribution::GrantKeyword(Keyword::Trample)),
            ],
            mana_colors: Vec::new(),
            text: String::new(),
        });

        let mut s = GameState::new(2, seed);
        s.set_card_db(std::sync::Arc::new(db));
        s
    }

    /// Compact render of the decisive events for a scenario (for expect traces).
    fn event_trace(events: &[GameEvent]) -> String {
        let mut out = String::new();
        for ev in events {
            let line = match ev {
                GameEvent::SpellCast { spell, controller } => {
                    Some(format!("{controller:?} casts {spell:?}"))
                }
                GameEvent::ObjectMoved { obj, to } => Some(format!("{obj:?} -> {to:?}")),
                GameEvent::DrewCards { player, count } => Some(format!("{player:?} draws {count}")),
                GameEvent::DamageDealt { target, amount, source } => {
                    Some(format!("{amount} dmg {source:?} -> {target:?}"))
                }
                GameEvent::LifeChanged { player, new_total, .. } => {
                    Some(format!("{player:?} life -> {new_total}"))
                }
                GameEvent::PermanentDied { obj } => Some(format!("{obj:?} dies")),
                GameEvent::GameEnded { winner } => Some(format!("game over: {winner:?}")),
                _ => None,
            };
            if let Some(l) = line {
                out.push_str(&l);
                out.push('\n');
            }
        }
        out
    }

    #[test]
    fn etb_trigger_draws_a_card() {
        // Elvish Visionary: "When this creature enters, draw a card." (CR 603.6a ETB trigger.)
        // Library card is a creature (not a land the aggro agent would then play), so the
        // drawn card stays in hand and the trigger's effect is observable.
        let mut state = cards::build_game(1, &[&[grp::GRIZZLY_BEARS], &[]]);
        put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield);
        put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield);
        let viz = put(&mut state, PlayerId(0), grp::ELVISH_VISIONARY, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);
        e.record_events(true);
        e.priority_round();

        assert_eq!(e.state.object(viz).zone, Zone::Battlefield, "Visionary entered");
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 1, "ETB trigger drew a card");
        assert!(e.state.player(PlayerId(0)).library.is_empty(), "drew the only library card");
        expect![[r#"
            PlayerId(0) casts StackId(1)
            ObjId(4) -> Battlefield
            PlayerId(0) draws 1
        "#]]
        .assert_eq(&event_trace(&e.event_log));
    }

    #[test]
    fn etb_trigger_targets_and_kills_a_creature() {
        // Flametongue Kavu: ETB deals 4 to target creature. The trigger targets when it goes
        // on the stack (603.3d); the aggro agent picks the enemy 2/2, which then dies to the
        // lethal-damage SBA (704.5g).
        let mut state = cards::build_game(2, &[&[], &[]]);
        let prey = put(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield);
        for _ in 0..4 {
            put(&mut state, PlayerId(0), grp::MOUNTAIN, Zone::Battlefield); // pay {3}{R}
        }
        let ftk = put(&mut state, PlayerId(0), grp::FLAMETONGUE_KAVU, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);
        e.record_events(true);
        e.priority_round();

        assert_eq!(e.state.object(ftk).zone, Zone::Battlefield, "FTK entered");
        assert_eq!(e.state.object(prey).zone, Zone::Graveyard, "FTK's ETB killed the enemy 2/2");
        expect![[r#"
            PlayerId(0) casts StackId(1)
            ObjId(6) -> Battlefield
            4 dmg ObjId(6) -> Object(ObjId(1))
            ObjId(1) dies
            ObjId(1) -> Graveyard
        "#]]
        .assert_eq(&event_trace(&e.event_log));
    }

    #[test]
    fn dies_trigger_draws_a_card() {
        // Exultant Cultist: "When this creature dies, draw a card." A lethal-damage SBA
        // destroys it → the SelfDies trigger fires (source found in the graveyard by grp_id)
        // → resolves to a draw.
        let mut state = cards::build_game(5, &[&[grp::GRIZZLY_BEARS], &[]]); // P0 lib: 1 card
        let cultist = put(&mut state, PlayerId(0), grp::EXULTANT_CULTIST, Zone::Battlefield);
        state.objects.get_mut(&cultist).unwrap().damage_marked = 2; // lethal for a 2/2
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = pass_engine(state);
        e.record_events(true);
        e.priority_round();

        assert_eq!(e.state.object(cultist).zone, Zone::Graveyard, "Cultist died");
        assert_eq!(e.state.player(PlayerId(0)).hand.len(), 1, "dies trigger drew a card");
        expect![[r#"
            ObjId(2) dies
            ObjId(2) -> Graveyard
            PlayerId(0) draws 1
        "#]]
        .assert_eq(&event_trace(&e.event_log));
    }

    #[test]
    fn enters_with_counters_replacement_keeps_a_0_0_alive() {
        // Servant of the Scale: a 0/0 that "enters with a +1/+1 counter". The whiteboard
        // rewrite pass turns its ETB into entering-with-a-counter, so it's a 1/1 that survives
        // the toughness-0 SBA. (Straight-through commit would let a 0/0 die immediately.)
        use crate::basics::CounterKind;
        let mut state = synth_state(3);
        put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield); // pay {G}
        let servant = put(&mut state, PlayerId(0), synth::COUNTER_CREATURE, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);
        e.priority_round();

        let s = e.state.object(servant);
        assert_eq!(s.zone, Zone::Battlefield, "Servant survived (entered with a counter)");
        assert_eq!(s.counters.get(&CounterKind::PlusOnePlusOne), 1, "entered with one +1/+1");
        assert_eq!(s.effective_power(), 1);
        assert_eq!(s.effective_toughness(), 1);
    }

    #[test]
    fn prevention_replacement_stops_combat_damage() {
        // Fog Bank (0/2) blocks a 2/2 Grizzly Bears. Its prevention replacement removes the
        // combat-damage event to it, so it takes 0 (would otherwise die), and its own 0 power
        // deals nothing to the attacker. No DamageDealt events occur.
        use crate::combat::{Attack, Block, CombatState};
        let mut state = synth_state(4);
        let bears = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let fog = put(&mut state, PlayerId(1), synth::FOG, Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.combat = Some(CombatState {
            attackers: vec![Attack {
                attacker: bears,
                defender: Target::Player(PlayerId(1)),
            }],
            blocks: vec![Block {
                blocker: fog,
                attacker: bears,
            }],
        });
        let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
        e.record_events(true);
        e.combat_damage();

        assert_eq!(e.state.object(fog).damage_marked, 0, "combat damage to Fog Bank prevented");
        assert_eq!(e.state.object(bears).damage_marked, 0, "Fog Bank's 0 power deals nothing");
        assert!(sba::collect(&e.state).is_empty(), "nothing dies");
        // No damage was dealt (the only candidate event was prevented).
        expect![[r#""#]].assert_eq(&event_trace(&e.event_log));
    }

    #[test]
    fn global_replacement_root_maze_taps_an_opponents_land() {
        // Root Maze ("Artifacts and lands enter tapped") is a GLOBAL replacement on P1's
        // enchantment that rewrites P0's land's ETB — validating the cross-battlefield scan.
        let mut state = cards::build_game(6, &[&[], &[]]);
        put(&mut state, PlayerId(1), grp::ROOT_MAZE, Zone::Battlefield);
        let forest = put(&mut state, PlayerId(0), grp::FOREST, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);
        e.priority_round();

        assert_eq!(e.state.object(forest).zone, Zone::Battlefield, "land was played");
        assert!(e.state.object(forest).status.tapped, "Root Maze made it enter tapped");
    }

    #[test]
    fn global_counter_modifier_hardened_scales_buffs_servant() {
        // Hardened Scales (GLOBAL) modifies the AddCounters event that Servant of the Scale's
        // own enters-with-a-counter replacement produces — a replacement modifying another
        // replacement's output, resolved by the fixpoint. 0/0 → enters with 2 counters → 2/2.
        use crate::basics::CounterKind;
        let mut state = synth_state(7);
        put(&mut state, PlayerId(0), grp::HARDENED_SCALES, Zone::Battlefield);
        put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield); // pay {G}
        let servant = put(&mut state, PlayerId(0), synth::COUNTER_CREATURE, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);
        e.priority_round();

        let s = e.state.object(servant);
        assert_eq!(s.zone, Zone::Battlefield);
        assert_eq!(
            s.counters.get(&CounterKind::PlusOnePlusOne),
            2,
            "1 (self) + 1 (Hardened Scales) = 2 counters"
        );
        assert_eq!(s.effective_toughness(), 2);
    }

    #[test]
    fn replacement_choice_616_1f_with_two_hardened_scales() {
        // TWO Hardened Scales both apply to Servant's one AddCounters event → CR 616.1f: the
        // affected object's controller chooses the order, then we re-check. Each adds 1, so
        // Servant ends with 1 + 1 + 1 = 3 counters; the ChooseReplacement decision is surfaced.
        use crate::basics::CounterKind;
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut state = synth_state(8);
        put(&mut state, PlayerId(0), grp::HARDENED_SCALES, Zone::Battlefield);
        put(&mut state, PlayerId(0), grp::HARDENED_SCALES, Zone::Battlefield);
        put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield); // pay {G}
        let servant = put(&mut state, PlayerId(0), synth::COUNTER_CREATURE, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(ReplacementSpy { seen: Rc::clone(&seen) }),
            Box::new(PassAgent),
        ];
        let mut e = Engine::new(state, agents);
        e.priority_round();

        assert_eq!(
            e.state.object(servant).counters.get(&CounterKind::PlusOnePlusOne),
            3,
            "both Hardened Scales applied (1 + 1 + 1)"
        );
        assert!(
            seen.borrow().iter().any(|&n| n == 2),
            "a ChooseReplacement with 2 applicable replacements was surfaced (CR 616.1f)"
        );
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

    /// A whole deterministic lands-only game (two seeded `RandomAgent`s, 8-card libraries)
    /// rendered as a turn-by-turn trace of its meaningful events: draws, land plays, and the
    /// decking loss that ends it (CR 704.5b). This snapshots the milestone-2 game loop.
    #[test]
    fn full_lands_only_game_trace() {
        let mut state = GameState::new(2, 7);
        for seat in 0..2u32 {
            for _ in 0..8 {
                state.add_card(PlayerId(seat), Characteristics::basic_land("Forest"), Zone::Library);
            }
        }
        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(RandomAgent::new(7 ^ 0xA11CE)),
            Box::new(RandomAgent::new(7 ^ 0xB0B)),
        ];
        let mut engine = Engine::new(state, agents);
        engine.record_events(true);
        let winner = engine.run_game();
        assert_eq!(winner, Some(PlayerId(0)));

        let mut out = String::new();
        let mut cur_turn = 0u32;
        for ev in &engine.event_log {
            match ev {
                GameEvent::PhaseBegan { turn, active, .. } if *turn != cur_turn => {
                    cur_turn = *turn;
                    out.push_str(&format!("== turn {turn} (active {active:?}) ==\n"));
                }
                GameEvent::PhaseBegan { .. } => {}
                GameEvent::DrewCards { player, count } => {
                    out.push_str(&format!("  {player:?} draws {count}\n"))
                }
                GameEvent::ObjectMoved { obj, to } => {
                    out.push_str(&format!("  {obj:?} -> {to:?}\n"))
                }
                GameEvent::GameEnded { winner } => {
                    out.push_str(&format!("game over, winner {winner:?}\n"))
                }
                other => out.push_str(&format!("  {other:?}\n")),
            }
        }
        expect![[r#"
              PlayerId(0) draws 7
              PlayerId(1) draws 7
            == turn 1 (active PlayerId(0)) ==
              ObjId(3) -> Battlefield
            == turn 2 (active PlayerId(1)) ==
              PlayerId(1) draws 1
              ObjId(9) -> Battlefield
            == turn 3 (active PlayerId(0)) ==
              PlayerId(0) draws 1
            == turn 4 (active PlayerId(1)) ==
            game over, winner Some(PlayerId(0))
        "#]]
        .assert_eq(&out);
    }

    /// Casting an instant: P0 Shocks P1 (the opponent player) for 2. Exercises legal-action
    /// enumeration of `Cast`, target choice (601.2c), auto-tap payment, the stack, resolution,
    /// and the `DealDamage` interpreter.
    #[test]
    fn cast_shock_damages_opponent() {
        let mut state = cards::build_game(1, &[&[], &[]]);
        let mountain = put(&mut state, PlayerId(0), grp::MOUNTAIN, Zone::Battlefield);
        let shock = put(&mut state, PlayerId(0), grp::SHOCK, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);
        e.priority_round();

        assert_eq!(e.state.player(PlayerId(1)).life, 18, "Shock dealt 2 to the opponent");
        assert_eq!(e.state.object(shock).zone, Zone::Graveyard, "instant to graveyard");
        assert!(e.state.object(mountain).status.tapped, "land tapped to pay {{R}}");
        assert!(e.state.stack.is_empty());
    }

    /// Lightning Bolt {R}: deals 3 to any target (here, the opponent's face).
    #[test]
    fn cast_lightning_bolt_to_the_face() {
        let mut state = cards::build_game(1, &[&[], &[]]);
        put(&mut state, PlayerId(0), grp::MOUNTAIN, Zone::Battlefield);
        put(&mut state, PlayerId(0), grp::LIGHTNING_BOLT, Zone::Hand);
        state.active_player = PlayerId(0);
        state.phase = Phase::PrecombatMain;
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);
        e.priority_round();
        assert_eq!(e.state.player(PlayerId(1)).life, 17, "Bolt dealt 3 to the opponent");
    }

    /// A creature that has been under control since the turn began can attack; an unblocked
    /// attacker deals its power to the defending player and is tapped.
    #[test]
    fn creature_attacks_unblocked_for_damage() {
        let mut state = cards::build_game(2, &[&[], &[]]);
        let bears = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        state.objects.get_mut(&bears).unwrap().summoning_sick = false;
        state.active_player = PlayerId(0);
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(PassAgent)]);

        e.run_step(Phase::DeclareAttackers);
        e.run_step(Phase::DeclareBlockers);
        e.run_step(Phase::CombatDamage);

        assert_eq!(e.state.player(PlayerId(1)).life, 18, "2/2 dealt 2 to the defender");
        assert!(e.state.object(bears).status.tapped, "attacker tapped (CR 508.1f)");
    }

    /// A 2/2 attacker blocked by a 2/2: both take 2 lethal damage and both are reported dead
    /// by the SBA check (CR 510.1c/d, 704.5g).
    #[test]
    fn blocked_attacker_and_blocker_trade() {
        use crate::combat::{Attack, Block, CombatState};
        use crate::sba::{self, DeathReason, StateBasedAction};

        let mut state = cards::build_game(3, &[&[], &[]]);
        let attacker = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let blocker = put(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield);
        state.active_player = PlayerId(0);
        state.combat = Some(CombatState {
            attackers: vec![Attack {
                attacker,
                defender: Target::Player(PlayerId(1)),
            }],
            blocks: vec![Block { blocker, attacker }],
        });
        let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
        e.combat_damage();

        assert_eq!(e.state.object(attacker).damage_marked, 2);
        assert_eq!(e.state.object(blocker).damage_marked, 2);
        assert_eq!(e.state.player(PlayerId(1)).life, 20, "blocked: no damage to the player");
        let sbas = sba::collect(&e.state);
        for id in [attacker, blocker] {
            assert!(sbas.contains(&StateBasedAction::CreatureDies {
                creature: id,
                reason: DeathReason::LethalDamage,
            }));
        }
    }

    /// A full deterministic R/G demo game (two `AggroAgent`s) rendered as a combat trace of
    /// its decisive events — casts, combat/burn damage, deaths, the lethal blow, game end.
    #[test]
    fn demo_game_combat_trace() {
        let state = cards::two_player_demo_game(11);
        let mut e = Engine::new(state, vec![Box::new(AggroAgent), Box::new(AggroAgent)]);
        e.record_events(true);
        e.run_game();

        let mut out = String::new();
        let mut cur_turn = 0u32;
        for ev in &e.event_log {
            match ev {
                GameEvent::PhaseBegan { turn, active, .. } if *turn != cur_turn => {
                    cur_turn = *turn;
                    out.push_str(&format!("T{turn} (active {active:?})\n"));
                }
                GameEvent::PhaseBegan { .. } | GameEvent::DrewCards { .. } => {}
                GameEvent::SpellCast { spell, controller } => {
                    out.push_str(&format!("  {controller:?} casts spell {spell:?}\n"))
                }
                GameEvent::DamageDealt { target, amount, .. } => {
                    out.push_str(&format!("  {amount} damage to {target:?}\n"))
                }
                GameEvent::LifeChanged { player, new_total, .. } => {
                    out.push_str(&format!("  {player:?} life -> {new_total}\n"))
                }
                GameEvent::PermanentDied { obj } => out.push_str(&format!("  {obj:?} dies\n")),
                GameEvent::GameEnded { winner } => {
                    out.push_str(&format!("game over, winner {winner:?}\n"))
                }
                _ => {}
            }
        }
        expect![[r#"
            T1 (active PlayerId(0))
              PlayerId(0) casts spell StackId(1)
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 18
            T2 (active PlayerId(1))
            T3 (active PlayerId(0))
              PlayerId(0) casts spell StackId(2)
            T4 (active PlayerId(1))
            T5 (active PlayerId(0))
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 16
            T6 (active PlayerId(1))
            T7 (active PlayerId(0))
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 14
            T8 (active PlayerId(1))
              PlayerId(1) casts spell StackId(3)
            T9 (active PlayerId(0))
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 12
            T10 (active PlayerId(1))
              3 damage to Player(PlayerId(0))
              PlayerId(0) life -> 17
            T11 (active PlayerId(0))
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 10
            T12 (active PlayerId(1))
              PlayerId(1) casts spell StackId(4)
              3 damage to Player(PlayerId(0))
              PlayerId(0) life -> 14
            T13 (active PlayerId(0))
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 8
            T14 (active PlayerId(1))
              PlayerId(1) casts spell StackId(5)
              2 damage to Player(PlayerId(0))
              PlayerId(0) life -> 12
              3 damage to Player(PlayerId(0))
              PlayerId(0) life -> 9
              2 damage to Player(PlayerId(0))
              PlayerId(0) life -> 7
            T15 (active PlayerId(0))
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 6
            T16 (active PlayerId(1))
              3 damage to Player(PlayerId(0))
              PlayerId(0) life -> 4
              2 damage to Player(PlayerId(0))
              PlayerId(0) life -> 2
            T17 (active PlayerId(0))
              PlayerId(0) casts spell StackId(6)
              2 damage to Player(PlayerId(1))
              PlayerId(1) life -> 4
            T18 (active PlayerId(1))
              3 damage to Player(PlayerId(0))
              PlayerId(0) life -> -1
              2 damage to Player(PlayerId(0))
              PlayerId(0) life -> -3
            game over, winner Some(PlayerId(1))
        "#]]
        .assert_eq(&out);
    }

    #[test]
    fn flash_lets_a_creature_be_cast_at_instant_speed() {
        // King Cheetah has flash → castable on the opponent's turn; a vanilla creature is not.
        let mut state = cards::build_game(1, &[&[], &[]]);
        for _ in 0..4 {
            put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield); // pay {3}{G} / {1}{G}
        }
        put(&mut state, PlayerId(0), grp::KING_CHEETAH, Zone::Hand); // flash
        put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Hand); // no flash
        state.active_player = PlayerId(1); // NOT P0's turn ⇒ sorcery speed unavailable to P0
        state.phase = Phase::PrecombatMain;
        let e = pass_engine(state);
        let casts: Vec<u32> = e
            .legal_priority_actions(PlayerId(0))
            .iter()
            .filter_map(|a| match a {
                PlayableAction::Cast { spell, .. } => Some(e.state.object(*spell).chars.grp_id),
                _ => None,
            })
            .collect();
        assert!(
            casts.contains(&grp::KING_CHEETAH),
            "flash creature is castable at instant speed (opponent's turn)"
        );
        assert!(
            !casts.contains(&grp::GRIZZLY_BEARS),
            "a non-flash creature is not castable on the opponent's turn"
        );
    }

    #[test]
    fn hexproof_blocks_opponent_targeting_only() {
        use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
        let mut state = cards::build_game(1, &[&[], &[]]);
        let scout = put(&mut state, PlayerId(1), grp::GLADECOVER_SCOUT, Zone::Battlefield); // hexproof
        let foe_bears = put(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let my_bears = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let e = pass_engine(state);
        let spec = TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        };
        // P0 (the opponent) cannot target the hexproof scout, but can target everything else.
        let cands = e.target_candidates(&spec, PlayerId(0));
        assert!(!cands.contains(&Target::Object(scout)), "opponent can't target hexproof");
        assert!(cands.contains(&Target::Object(foe_bears)), "opponent can target non-hexproof");
        assert!(cands.contains(&Target::Object(my_bears)), "own creature is targetable");
        // The controller of the hexproof creature can still target it.
        let own = e.target_candidates(&spec, PlayerId(1));
        assert!(own.contains(&Target::Object(scout)), "controller can target own hexproof");
    }

    #[test]
    fn indestructible_survives_destroy() {
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::{Effect, EffectTarget};
        let mut state = cards::build_game(1, &[&[], &[]]);
        let myr = put(&mut state, PlayerId(0), grp::DARKSTEEL_MYR, Zone::Battlefield); // indestructible
        let bears = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let mut e = pass_engine(state);
        let destroy = Effect::Destroy { what: EffectTarget::ChosenIndex(0) };

        // "Destroy" the indestructible artifact creature → it stays on the battlefield.
        e.resolve_effect(
            &destroy,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(myr)],
                ..Default::default()
            },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        assert_eq!(
            e.state.object(myr).zone,
            Zone::Battlefield,
            "indestructible creature survives a destroy effect"
        );

        // The same effect destroys a normal creature.
        e.resolve_effect(
            &destroy,
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(bears)],
                ..Default::default()
            },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        assert_eq!(
            e.state.object(bears).zone,
            Zone::Graveyard,
            "a creature without indestructible is destroyed"
        );
    }
}
