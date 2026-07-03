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
    NumberReason, ObjView, PlayableAction, PlayerView, SelectReason, StopStateView, TargetSlot,
};
use crate::basics::{CardType, CounterKind, ManaCost, Phase, Target, Zone, ZonePos};
use crate::subtypes::{EnchantmentType, Subtype};
use crate::effects::ability::{
    Ability, Cost, CostComponent, EventPattern, Keyword, Restriction, StaticContribution, Timing,
};
use crate::effects::action::{Action, MoveCause, ResolutionCtx, Whiteboard, WbReason};
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::ids::{ObjId, PlayerId, StackId};
use crate::mana;
use crate::replay::{Replay, ReplayFrame, ReplayMeta, ReplaySource};
use crate::sba::{self, LossReason, StateBasedAction};
use crate::stack::{StackObject, StackObjectKind};
use crate::state::view::{god_view, view_for};
use crate::state::GameState;
use crate::turn::{is_main_phase, step_grants_priority, TURN_STEPS};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// A hard cap on turns so a pathological game can never loop forever. Real games end far
/// sooner (a lands-only game ends when a player decks out, CR 704.5b). Reaching the cap
/// ends the game as a draw.
const MAX_TURNS: u32 = 2000;

/// Hard safety ceilings for the engine's two unbounded fixpoint loops (#55). A wedged game — an SBA
/// that never clears, a triggered ability that re-queues itself, a free repeatable action — would
/// otherwise spin a CPU core forever and hang training (a single `env.step()` never returns). These
/// are *absurdly* high relative to any legal game (a real phase settles in a handful of iterations),
/// so they never fire in correct play; when one does, the engine aborts the game to a draw and logs
/// the loop. Bounding the loops, not wall-clock, keeps the engine deterministic/replayable.
const AGENDA_LOOP_LIMIT: usize = 100_000;
const PRIORITY_LOOP_LIMIT: usize = 1_000_000;
/// Ceiling for the replacement/prevention rewrite fixpoint (`whiteboard::rewrite`, CR 614/616),
/// which runs inside `commit`/`resolve_top` — i.e. *below* the priority/agenda loops, so it needs
/// its own guard. Bounded by distinct (source, ability, affected) triples in legal play (small); a
/// pathological object-creating replacement chain is what this catches.
pub(crate) const REWRITE_LOOP_LIMIT: usize = 100_000;

/// Why the game ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// **The** stop knob — the whole stop policy: stop at every priority window (full control ON)
    /// vs the fixed elision rule (OFF — auto-pass unless you have a meaningful, non-mana action at a
    /// marked stop or with an opponent's object on the stack). See [`Engine::should_auto_pass`].
    /// (The old auto_pass/smart_stops/resolve_own_stack knobs were collapsed into this one.)
    pub full_control: bool,
    /// Per-step override of the Arena default, keyed by `(step, own_turn)` so the two sides of a
    /// step are independent (e.g. stop on *my* draw but not the opponent's). `own_turn` =
    /// `seat == active_player`. `Some(true)` = always stop here, `Some(false)` = never, absent =
    /// the Arena default.
    overrides: std::collections::BTreeMap<(Phase, bool), bool>,
    /// Manual mana (a human/UI session): when ON, the engine offers `ActivateMana` actions at
    /// priority so the seat can tap specific sources for mana (CR 605.3a) — e.g. to control which
    /// lands fund a spell. Default OFF: headless/agent seats auto-pay, so mana abilities never enter
    /// their action space, and these actions never count toward a SmartStop (see `priority_round`).
    pub manual_mana: bool,
}

impl Default for StopConfig {
    fn default() -> Self {
        StopConfig {
            // Headless/replay/tests default to full control = prompt every priority window
            // (deterministic; matches the old paper-CR default). UI/gym seats turn it OFF for the
            // fixed elision rule (web sets `full_control` directly; the gym via `set_arena_auto_pass`).
            full_control: true,
            overrides: std::collections::BTreeMap::new(),
            manual_mana: false, // agent/replay seats auto-pay; a UI session turns it on
        }
    }
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

/// The engine core: full [`GameState`] plus (transitionally) one [`Agent`] per seat (indexed by
/// `PlayerId.0`). Being renamed from `Engine` as the first step of the resumable split
/// (RESUMABLE_ENGINE.md M3.1): the game-logic methods live here and will run inside a fiber once
/// the driver is separated in M3.2. `agents` is still held here for now (removed in M3.2 when
/// `ask` yields to the driver instead of calling agents directly).
pub struct EngineCore {
    pub state: GameState,
    agents: Vec<Box<dyn Agent>>,
    /// Append-only record of every public event broadcast this game (the same stream sent
    /// to agents' `observe`). Handy for a CLI trace and for snapshot tests. Off by default.
    pub event_log: Vec<GameEvent>,
    record_events: bool,
    /// The permanents found by a `Search` during the **current** effect resolution — so a follow-up
    /// effect can reference "that land/creature" (Fabled Passage's "untap that land"). Cleared at
    /// the start of each `resolve_effect`; read by `EffectTarget::Searched`.
    pub(crate) searched_this_resolution: Vec<ObjId>,
    /// The object currently being iterated by an `Effect::ForEach` — bound while its body
    /// interprets, read by `EffectTarget::Each` (Dyadrine's "remove a counter from each of …").
    pub(crate) foreach_current: Option<ObjId>,
    /// When on, capture an omniscient [`ReplayFrame`] (a [`crate::replay::GodView`] + label) at
    /// each public event — the recorded replay stream (REPLAY_PLAN). Off by default.
    record_replay: bool,
    replay_frames: Vec<ReplayFrame>,
    /// Provenance stamped into the emitted [`Replay`]'s metadata (caller-set). Default `Human`.
    replay_source: ReplaySource,
    /// Optional live frame sink: called with each [`ReplayFrame`] as it is captured, so a caller
    /// can stream god-view frames to spectators mid-game (the engine runs synchronously, so this
    /// is the re-entrant point). Runs on the game thread. `None` = no live streaming.
    replay_sink: Option<Box<dyn FnMut(&ReplayFrame)>>,
    started: bool,
    /// One [`StopConfig`] per seat (incl. its `auto_pass` flag), behind `Arc<Mutex<…>>` so a UI session can hold a live
    /// handle ([`Engine::stops_handle`]) and toggle a seat's stops *mid-game* from another
    /// thread; the engine re-reads the config at every priority window. RL/headless play never
    /// touches these (auto-pass stays off), so the lock is uncontended there.
    stops: Vec<Arc<Mutex<StopConfig>>>,
    /// The resumable-step seam (RESUMABLE_ENGINE.md §3.2). When the game runs inside a [`Session`]
    /// fiber this holds a pointer to that fiber's `Yielder`, and [`EngineCore::ask`] **suspends**
    /// (yielding the decision to the driver) instead of calling an in-core agent. `None` on the
    /// blocking path (direct `run_game` / direct-call unit tests), where `ask` calls `agents`.
    /// A raw pointer because the `Yielder` lives on the fiber's own stack; it is only ever
    /// dereferenced from `ask` while that fiber is running (so never dangling). Set/cleared by
    /// [`EngineCore::run_in_fiber`].
    ///
    /// [`Session`]: crate::session::Session
    yielder: Option<*const corosensei::Yielder<DecisionResponse, crate::session::Step>>,
}

/// Transitional alias kept while the resumable split lands (RESUMABLE_ENGINE.md M3.1→M3.2): the
/// public name stays `Engine` so every `impl Engine` / `Engine::new` site (here, combat.rs,
/// whiteboard.rs, and the mtg-cli/gre-server/py crates) keeps working unchanged. In M3.2 `Engine`
/// becomes the distinct blocking driver `{ core: EngineCore, agents }` and this alias is dropped.
pub type Engine = EngineCore;

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
            searched_this_resolution: Vec::new(),
            foreach_current: None,
            record_replay: false,
            replay_frames: Vec::new(),
            replay_source: ReplaySource::Human,
            replay_sink: None,
            started: false,
            stops,
            yielder: None,
        }
    }

    /// Run this core's whole game **inside a fiber**, suspending at each decision (RESUMABLE_ENGINE.md
    /// §3.2). Sets the [`yielder`](Self::yielder) seam so `ask` yields to the driver, plays the game,
    /// then clears the seam and returns the finished core (for outcome/state inspection). Called only
    /// by [`Session`](crate::session::Session); the pointer is valid for the whole call because the
    /// `Yielder` lives on this fiber's stack.
    pub(crate) fn run_in_fiber(
        mut self,
        yielder: &corosensei::Yielder<DecisionResponse, crate::session::Step>,
    ) -> Self {
        self.yielder = Some(yielder as *const _);
        self.run_game();
        self.yielder = None; // never leave a dangling stack pointer in the returned core
        self
    }

    // ── Stop policy (one knob: full control vs the fixed rule; MTGA-style elision) ───────────

    /// Enable/disable the Arena auto-pass profile for *every* seat — now just the inverse of full
    /// control: `on` ⇒ the fixed elision rule (full control OFF), `off` ⇒ prompt every window (full
    /// control ON). Kept as a back-compat entry point (the gym + CLI call it); the separate
    /// `auto_pass`/`smart_stops`/`resolve_own_stack` knobs were collapsed into `full_control`
    /// (see [`Engine::should_auto_pass`]). Slated for removal once callers move to `set_full_control`.
    pub fn set_arena_auto_pass(&mut self, on: bool) {
        for cfg in &self.stops {
            cfg.lock().unwrap().full_control = !on; // auto-pass profile on ⇔ not full control
        }
    }
    /// "Full control" for a seat: stop at every priority window. This is THE stop knob — off =
    /// the fixed elision rule (auto-pass unless you have a meaningful action at a marked stop or an
    /// opponent's object is on the stack). See [`Engine::should_auto_pass`].
    pub fn set_full_control(&mut self, p: PlayerId, on: bool) {
        self.stops[p.0 as usize].lock().unwrap().full_control = on;
    }
    /// Manual mana for a seat (a human/UI session): ON = offer `ActivateMana` actions at priority so
    /// the seat can tap specific sources for mana (CR 605.3a). Default OFF keeps agent/replay seats
    /// on auto-pay (mana abilities stay out of their action space). See [`StopConfig::manual_mana`].
    pub fn set_manual_mana(&mut self, p: PlayerId, on: bool) {
        self.stops[p.0 as usize].lock().unwrap().manual_mana = on;
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

    /// The single stop policy — **one knob: full control vs not** (the former separate
    /// `auto_pass`/`smart_stops`/`resolve_own_stack` are collapsed away; they no longer affect
    /// behavior). Should `p`'s current priority window be auto-passed (elided) instead of prompting?
    ///
    /// - **Full control** → never auto-pass: stop at every priority window.
    /// - **Otherwise** → the fixed rule: auto-pass *unless* `p` has a meaningful (non-mana) action
    ///   AND either an opponent's object is on top of the stack (a window to respond) or it's a
    ///   marked-stop step on an empty stack. `has_meaningful` excludes mana abilities (#36) — being
    ///   able to tap a land is never itself a reason to stop.
    ///
    /// This is exactly the rule the web client used to apply on top of the engine; it now lives in
    /// the engine (CR-correct masking is the engine's job). Headless/replay/tests default to full
    /// control (prompt every window — deterministic), so their streams are unchanged.
    fn should_auto_pass(&self, p: PlayerId, has_meaningful: bool) -> bool {
        let cfg = self.stops[p.0 as usize].lock().unwrap();
        if cfg.full_control {
            return false; // stop at every priority window
        }
        if !has_meaningful {
            return true; // nothing but mana taps (or nothing at all) → auto-pass
        }
        let opp_on_top = self.state.stack.top().is_some_and(|t| t.controller != p);
        let marked_step_stop =
            self.state.stack.is_empty() && cfg.stops_at(p, self.state.phase, self.state.active_player);
        !(opp_on_top || marked_step_stop)
    }

    /// Enable recording of broadcast events into [`Engine::event_log`] (for tracing/tests).
    pub fn record_events(&mut self, on: bool) {
        self.record_events = on;
    }

    // ── Replay recording (REPLAY_PLAN — omniscient snapshots) ────────────────────────────────

    /// Enable/disable replay recording. When turned on, the engine captures an omniscient
    /// [`ReplayFrame`] (a [`crate::replay::GodView`] + label) at every public event, so a live
    /// spectator can be fed the same frames and a finished game can be saved. Enabling captures an
    /// initial "game start" frame of the current state.
    pub fn record_replay(&mut self, on: bool) {
        self.record_replay = on;
        if on && self.replay_frames.is_empty() {
            self.push_replay_frame("game start".to_string());
        }
    }

    /// Set the provenance stamped into the emitted [`Replay`] metadata (default `Human`).
    pub fn set_replay_source(&mut self, source: ReplaySource) {
        self.replay_source = source;
    }

    /// Install a live frame sink: `sink` is invoked with each [`ReplayFrame`] the moment it is
    /// captured, so a caller can stream god-view frames to spectators *during* the synchronous
    /// game run (e.g. forward each frame onto a broadcast channel). Implies replay recording is
    /// on (turns it on if it wasn't, capturing the initial "game start" frame). Replaces any
    /// previously-installed sink.
    pub fn set_replay_sink(&mut self, sink: Box<dyn FnMut(&ReplayFrame)>) {
        self.replay_sink = Some(sink);
        self.record_replay(true);
    }

    /// Capture one omniscient frame of the *current* state, labelled with what just happened, and
    /// (if a [sink][Engine::set_replay_sink] is installed) stream it live. No-op unless recording.
    fn push_replay_frame(&mut self, label: String) {
        if !self.record_replay {
            return;
        }
        let state = god_view(&self.state);
        let frame = ReplayFrame { state, label };
        if let Some(sink) = self.replay_sink.as_mut() {
            sink(&frame);
        }
        self.replay_frames.push(frame);
    }

    /// The replay accumulated so far — callable incrementally (e.g. to feed a live spectator) and
    /// after the game. The engine fills the seats and, once the game is over, the [`Outcome`];
    /// the caller overwrites `source`/`created_at`/player names+decks (stamped from outside).
    pub fn replay(&self) -> Replay {
        let mut meta = ReplayMeta::new(self.state.players.len(), self.replay_source.clone());
        if self.state.game_over {
            meta.result = Some(self.outcome());
        }
        Replay { meta, frames: self.replay_frames.clone() }
    }

    /// How many replay frames have been captured so far (for incremental spectator streaming).
    pub fn replay_frame_count(&self) -> usize {
        self.replay_frames.len()
    }

    /// A short human label for the replay frame produced by `ev` ("what just happened").
    fn event_label(&self, ev: &GameEvent) -> String {
        let name = |id: ObjId| -> String {
            self.state
                .objects
                .get(&id)
                .map(|o| o.chars.name.clone())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| format!("{id:?}"))
        };
        match ev {
            GameEvent::PhaseBegan { turn, phase, active } => {
                format!("Turn {turn} — P{} {phase:?}", active.0)
            }
            GameEvent::DrewCards { player, count } => format!("P{} draws {count}", player.0),
            GameEvent::LifeChanged { player, delta, new_total } => {
                format!("P{} life {delta:+} → {new_total}", player.0)
            }
            GameEvent::DamageDealt { target, amount, source } => {
                format!("{} deals {amount} damage to {target:?}", name(*source))
            }
            GameEvent::SpellCast { spell, controller } => {
                let sname = self
                    .state
                    .stack
                    .items
                    .iter()
                    .find(|s| s.id == *spell)
                    .and_then(|s| match s.kind {
                        StackObjectKind::Spell(obj) => Some(name(obj)),
                        _ => None,
                    })
                    .unwrap_or_else(|| "a spell".to_string());
                format!("P{} casts {sname}", controller.0)
            }
            GameEvent::ObjectMoved { obj, to } => format!("{} → {to:?}", name(*obj)),
            GameEvent::PermanentDied { obj } => format!("{} dies", name(*obj)),
            GameEvent::Revealed { to, objects } => {
                format!("P{} is shown {} card(s)", to.0, objects.len())
            }
            GameEvent::ValueChosen { player, label, value } => {
                format!("P{} {label} = {value}", player.0)
            }
            GameEvent::Targeted { object, by } => {
                format!("{} targeted by P{}", name(*object), by.0)
            }
            GameEvent::AttackersDeclared { attackers, by } => {
                format!("P{} attacks with {} creature(s)", by.0, attackers.len())
            }
            GameEvent::GameEnded { winner } => match winner {
                Some(w) => format!("Game over — P{} wins", w.0),
                None => "Game over — draw".to_string(),
            },
            // Not recorded (gated in `broadcast`); labelled for completeness only.
            GameEvent::ManaPoolChanged { player } => format!("P{} mana pool changed", player.0),
        }
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
        // CR 103.2–103.4: a randomly-chosen player decides who takes the first turn.
        self.choose_starting_player(&seats);
        // Opening draws are not "draw step" draws and don't risk decking on a normal deck.
        for &p in &seats {
            self.draw(p, hand_size);
        }
        self.run_mulligans(&seats, hand_size);
    }

    /// CR 103.2–103.4: a player is randomly chosen (the opening die roll / coin flip) to decide
    /// who takes the first turn, and that player then chooses the starting player (themselves or
    /// another). The die roll uses the seeded engine RNG (replayable); the choice flows through
    /// the decider's `Agent` via `ChooseStartingPlayer`. Sets `active_player` to the result.
    fn choose_starting_player(&mut self, seats: &[PlayerId]) {
        if seats.is_empty() {
            return;
        }
        // The die roll (CR 103.2): randomly pick which player decides.
        let decider = seats[self.state.rng.below(seats.len() as u64) as usize];
        let req = DecisionRequest::ChooseStartingPlayer {
            candidates: seats.to_vec(),
        };
        // The decider chooses who takes the first turn (CR 103.4).
        let starting = match self.ask(decider, &req) {
            DecisionResponse::Index(i) => *seats.get(i as usize).unwrap_or(&decider),
            _ => decider,
        };
        self.state.active_player = starting;
        // The starting player anchors turn rotation + the CR 103.8a first-draw skip.
        self.state.starting_player = starting;
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
        // "gained life this turn" (CR 118.9) and "a card left your graveyard this turn" are per-turn —
        // reset them for every player at turn start.
        for i in 0..self.state.players.len() {
            self.state.players[i].life_gained_this_turn = 0;
            self.state.players[i].cards_left_graveyard_this_turn = 0;
        }
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
        // (2) Remove all marked damage; end "until end of turn"/"this turn" effects (514.2),
        // simultaneously: floating continuous effects (e.g. a +X/+0 pump) and marked damage.
        self.state.end_of_turn_continuous_cleanup();
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
        let mut iters = 0usize;

        loop {
            if self.loop_guard_tripped(iters, PRIORITY_LOOP_LIMIT, "priority_round") {
                self.state.priority_player = None;
                return;
            }
            iters += 1;
            self.run_agenda();
            if self.state.game_over {
                self.state.priority_player = None;
                return;
            }
            let p = order[idx];
            self.state.priority_player = Some(p);

            let actions = self.legal_priority_actions(p);
            // Mana abilities (CR 605) never warrant a SmartStop on their own — you can always tap a
            // land, so counting them would force a prompt every step. Key the auto-pass decision off
            // the *meaningful* (non-mana) actions; the mana actions still ride along in the prompt
            // when the seat IS stopped, so a human can tap specific sources before paying (#36).
            let has_meaningful =
                actions.iter().any(|a| !matches!(a, PlayableAction::ActivateMana { .. }));
            // Arena-profile auto-pass (AGENT_INTERFACE §8.1): elide this window (treat as a
            // pass without prompting the agent) when the policy says so. Off ⇒ always prompt.
            let response = if self.should_auto_pass(p, has_meaningful) {
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

        // Play a land (CR 116.2a / 505.6b): up to 1 + extra-land-play permissions per turn, from
        // your hand (and any other zone a `PlayLandsFrom` permission opens, e.g. graveyard).
        if sorcery_speed && s.player(p).lands_played_this_turn < 1 + self.extra_land_plays(p) {
            for &card in &s.player(p).hand {
                if s.object(card).chars.is_land() {
                    actions.push(PlayableAction::PlayLand { card });
                }
            }
            for zone in [Zone::Graveyard, Zone::Exile] {
                if self.can_play_lands_from(p, zone) {
                    let cards: Vec<ObjId> = s
                        .player(p)
                        .zone_ids(zone)
                        .iter()
                        .copied()
                        .filter(|&card| s.object(card).chars.is_land())
                        .collect();
                    for card in cards {
                        actions.push(PlayableAction::PlayLand { card });
                    }
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
            // Instants and Flash (CR 702.8) cast at instant speed; everything else sorcery-speed.
            let instant_speed = chars.has_type(CardType::Instant) || chars.keywords.contains(&Keyword::Flash);
            let timing_ok = instant_speed || sorcery_speed;
            if !timing_ok {
                continue;
            }
            // Must be able to choose legal targets (CR 601.2c) — modal-aware, and Aura-aware (an
            // Aura needs a legal permanent to enchant, else it deadlocks the target decision).
            if !self.card_castable_targets(card, p) {
                continue;
            }
            // Normal cast for the mana cost.
            if chars.mana_cost.as_ref().is_some_and(|c| mana::can_pay(s, p, c)) {
                actions.push(PlayableAction::Cast { spell: card, variant: CastVariant::Normal });
            }
            // Warp (CR 702.x): an alternative cast cost from hand at sorcery speed — offered even
            // when the normal cost is unaffordable (the discount is the whole point).
            if sorcery_speed {
                if let Some(wcost) = self.warp_cost(card) {
                    if mana::can_pay(s, p, &wcost) {
                        actions.push(PlayableAction::Cast { spell: card, variant: CastVariant::Warp });
                    }
                }
            }
        }

        // Cast a warp-exiled card from exile on a later turn (CR 702.x), at sorcery speed for its
        // normal mana cost — the warp recast.
        if sorcery_speed {
            for &card in &s.player(p).exile {
                let o = s.object(card);
                if !o.castable_from_exile {
                    continue;
                }
                let affordable = o.chars.mana_cost.as_ref().is_some_and(|c| mana::can_pay(s, p, c));
                if !affordable {
                    continue;
                }
                if self.card_castable_targets(card, p) {
                    actions.push(PlayableAction::Cast { spell: card, variant: CastVariant::Normal });
                }
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

        // Manual mana abilities (CR 605.3a). Offered ONLY to a seat with manual mana on (a UI
        // session): one `ActivateMana` per untapped usable source, so a human can tap specific
        // lands to control which sources fund a spell. Headless/agent seats leave this off and
        // auto-pay, so these never enter the agent's action space. They also don't warrant a
        // SmartStop on their own (`priority_round` keys the stop decision off non-mana actions).
        if self.stops[p.0 as usize].lock().unwrap().manual_mana {
            for (source, _colors) in mana::usable_mana_sources(s, p) {
                // The source's authored `{T}: Add …` ability index, if any; else a sentinel for
                // intrinsic basic-land-type mana (CR 305.6, no authored ability). Execution
                // recomputes colours from the source, so this is only a label hint.
                let ability = s
                    .def_of(source)
                    .and_then(|d| {
                        d.abilities
                            .iter()
                            .position(|ab| matches!(ab, Ability::Activated { is_mana: true, .. }))
                    })
                    .map(|i| AbilityRef(i as u32))
                    .unwrap_or(AbilityRef(u32::MAX));
                actions.push(PlayableAction::ActivateMana { source, ability });
            }
        }
        actions
    }

    /// Whether `p` can pay `cost` to activate an ability of `source`. Handles the components the
    /// starter set uses (mana, `{T}`); other components aren't masked yet and pass through.
    fn can_pay_cost(&self, p: PlayerId, source: ObjId, cost: &Cost) -> bool {
        if let Some(m) = &cost.mana {
            // Exclude sources committed to a non-mana component from the mana check (they'll be
            // tapped/sacrificed before mana is paid, so they can't also produce it) — #57. `{T}`
            // commits the source; that's the case that bit Ba Sing Se.
            let excluded: Vec<ObjId> = cost
                .components
                .iter()
                .any(|c| matches!(c, CostComponent::TapSelf))
                .then_some(source)
                .into_iter()
                .collect();
            if !mana::can_pay_excluding(&self.state, p, m, &excluded) {
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
                // Sacrifice (CR 118.3 / 701.17): payable iff the chooser controls enough matching
                // permanents (e.g. `{T}, Sacrifice this:` needs the source itself on the field).
                CostComponent::Sacrifice(spec) => {
                    self.sacrifice_candidates(p, source, spec).len() as u32 >= cost_count(&spec.min)
                }
                // Crew N (CR 702.122): payable iff untapped creatures you control total power ≥ N.
                CostComponent::Crew(n) => {
                    let total: i32 = self
                        .crew_candidates(p, source)
                        .iter()
                        .map(|&id| self.crew_power(id))
                        .sum();
                    total >= *n as i32
                }
                _ => true,
            };
            if !ok {
                return false;
            }
        }
        true
    }

    /// Extra land plays `p` is granted by `StaticContribution::ExtraLandPlays` permissions on the
    /// permanents they control (Exploration / Azusa / Icetill) — beyond the base one (CR 505.5b).
    fn extra_land_plays(&self, p: PlayerId) -> u32 {
        self.player_static_permissions(p)
            .filter_map(|c| match c {
                StaticContribution::ExtraLandPlays(n) => Some(*n),
                _ => None,
            })
            .sum()
    }

    /// Whether `p` may play lands from `zone` (a `PlayLandsFrom` permission, e.g. Crucible / Icetill
    /// "play lands from your graveyard").
    fn can_play_lands_from(&self, p: PlayerId, zone: Zone) -> bool {
        self.player_static_permissions(p)
            .any(|c| matches!(c, StaticContribution::PlayLandsFrom(z) if *z == zone))
    }

    /// Every `StaticContribution` from the printed `Ability::Static`s of permanents `p` controls —
    /// for the player-level permissions (land plays) read directly here, not painted on objects.
    fn player_static_permissions(&self, p: PlayerId) -> impl Iterator<Item = &StaticContribution> {
        self.state.player(p).battlefield.iter().flat_map(move |&id| {
            self.state
                .def_of(id)
                .into_iter()
                .flat_map(|def| def.abilities.iter())
                .filter_map(|ab| match ab {
                    Ability::Static { contribution, .. } => Some(contribution),
                    _ => None,
                })
        })
    }

    fn perform_priority_action(&mut self, p: PlayerId, action: &PlayableAction) {
        match action {
            PlayableAction::PlayLand { card } => self.play_land(p, *card),
            PlayableAction::Cast { spell, variant } => self.cast_spell(p, *spell, *variant),
            PlayableAction::Activate { source, ability } => {
                self.activate_ability(p, *source, *ability)
            }
            // Mana abilities resolve immediately without the stack (CR 605.3b).
            PlayableAction::ActivateMana { source, .. } => self.activate_mana_ability(p, *source),
            // Special actions (CR 116) — none routed through here yet.
            PlayableAction::Special { .. } => {}
        }
    }

    /// Manually activate a mana ability (CR 605.3 — no stack): tap `source` for one mana, asking the
    /// controller which colour when it can produce more than one. The mana floats into the pool
    /// (CR 106.4, emptied at end of step). The seat retains priority (it acted), so it can tap
    /// several sources in a row before paying a cost — letting a human choose which lands fund a
    /// spell (#36). A no-op if `source` isn't a current usable mana source for `p`.
    fn activate_mana_ability(&mut self, p: PlayerId, source: ObjId) {
        let colors = match mana::usable_mana_sources(&self.state, p)
            .into_iter()
            .find(|(id, _)| *id == source)
        {
            Some((_, cs)) => cs,
            None => return,
        };
        let color = if colors.len() == 1 {
            colors[0]
        } else {
            // Multi-colour source (a dual / any-colour): ask which colour to make (CR 605.3a).
            let resp = self.ask(
                p,
                &DecisionRequest::ChooseColor { allowed: colors.clone(), min: 1, max: 1 },
            );
            match resp {
                DecisionResponse::Indices(v) => {
                    v.first().and_then(|&i| colors.get(i as usize)).copied().unwrap_or(colors[0])
                }
                DecisionResponse::Index(i) => colors.get(i as usize).copied().unwrap_or(colors[0]),
                _ => colors[0],
            }
        };
        if mana::produce_mana(&mut self.state, p, source, color) {
            // Live-view refresh so the client shows the mana entering the pool (#62).
            self.broadcast(GameEvent::ManaPoolChanged { player: p });
        }
    }

    /// Activate a (non-mana) activated ability (CR 602.2): put it on the stack, choose targets
    /// (locked now, 602.2b), then pay the cost. It resolves via [`Engine::resolve_top`].
    pub(crate) fn activate_ability(&mut self, p: PlayerId, source: ObjId, ability: AbilityRef) {
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
            // Modal activated abilities would choose modes here (602.2b); none in the pool yet.
            modes: Vec::new(),
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
                source: Some(source),
                slots: slots.clone(),
            };
            let resp = self.ask(p, &req);
            let chosen = parse_targets(&slots, &resp);
            let targeted = self.targeted_object_ids(&chosen);
            if let Some(obj) = self.state.stack.items.iter_mut().find(|s| s.id == sid) {
                obj.targets = chosen;
            }
            // CR 603.2: each targeted object becomes the target of this activated ability.
            self.fire_targeted(&targeted, p);
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
        // Pay NON-mana components FIRST (CR 601.2h — the payer orders cost payment): committing them
        // first means a source tapped/sacrificed for a `{T}`/Sacrifice cost is already excluded from
        // the mana sources, so it can't ALSO produce mana for the same cost (Ba Sing Se's `{T}` can't
        // pay both its own tap AND a `{G}` of its `{2}{G}`). #57/#59.
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
                CostComponent::Sacrifice(spec) => self.pay_sacrifice(p, source, spec),
                CostComponent::Crew(n) => self.pay_crew(p, source, *n),
                _ => {}
            }
        }
        // Then pay mana from the REMAINING (still-untapped, still-present) sources, through the pool.
        if let Some(m) = &cost.mana {
            mana::auto_pay(&mut self.state, p, m);
            // Live-view refresh so the client sees the pool change (produce + spend) as it happens.
            self.broadcast(GameEvent::ManaPoolChanged { player: p });
        }
    }

    /// The untapped creatures `p` controls that could crew a Vehicle (CR 702.122) — excludes the
    /// Vehicle itself (it isn't a creature until crewed).
    fn crew_candidates(&self, p: PlayerId, source: ObjId) -> Vec<ObjId> {
        self.state
            .player(p)
            .battlefield
            .iter()
            .copied()
            .filter(|&id| {
                id != source
                    && self.state.objects.get(&id).is_some_and(|o| !o.status.tapped)
                    && self.state.computed(id).is_creature()
            })
            .collect()
    }

    fn crew_power(&self, id: ObjId) -> i32 {
        self.state.computed(id).power.unwrap_or(0).max(0)
    }

    /// Pay Crew N (CR 702.122): the controller taps a chosen subset of its untapped creatures with
    /// total power ≥ N. `can_pay_cost` already guaranteed a sufficient set exists; if the agent
    /// under-picks, greedily top up so the cost is met.
    fn pay_crew(&mut self, payer: PlayerId, source: ObjId, need: u32) {
        let candidates = self.crew_candidates(payer, source);
        let idxs = match self.ask(
            payer,
            &DecisionRequest::SelectCards {
                reason: SelectReason::Generic,
                from: candidates.clone(),
                min: 0,
                max: candidates.len() as u32,
                description: format!("tap creatures with total power {need}+ to crew"),
            },
        ) {
            DecisionResponse::Indices(i) => {
                self.distinct_valid_indices(&i, candidates.len(), candidates.len() as u32)
            }
            _ => Vec::new(),
        };
        let mut chosen: Vec<ObjId> = idxs.into_iter().map(|i| candidates[i]).collect();
        let mut total: i32 = chosen.iter().map(|&id| self.crew_power(id)).sum();
        if total < need as i32 {
            for &id in &candidates {
                if total >= need as i32 {
                    break;
                }
                if !chosen.contains(&id) {
                    chosen.push(id);
                    total += self.crew_power(id);
                }
            }
        }
        if total >= need as i32 {
            for id in chosen {
                if let Some(o) = self.state.objects.get_mut(&id) {
                    o.status.tapped = true;
                }
            }
        }
    }

    /// The permanents `payer` could sacrifice to pay a [`CostComponent::Sacrifice`] (CR 701.17):
    /// battlefield permanents the chooser controls that match the spec's filter. `ItSelf` resolves
    /// to the cost's `source`, so `{T}, Sacrifice this:` yields just the source.
    fn sacrifice_candidates(&self, payer: PlayerId, source: ObjId, spec: &SelectSpec) -> Vec<ObjId> {
        let chooser = match spec.chooser {
            PlayerRef::Opponent | PlayerRef::EachOpponent => self
                .state
                .players
                .iter()
                .map(|x| x.id)
                .find(|&q| q != payer)
                .unwrap_or(payer),
            _ => payer,
        };
        self.state
            .player(chooser)
            .battlefield
            .iter()
            .copied()
            .filter(|&o| self.sac_filter_matches(o, &spec.filter, source, payer))
            .collect()
    }

    /// Source-aware filter match for a sacrifice candidate — like [`Engine::enter_filter_matches`]
    /// but resolves `ItSelf` against the cost's `source` (so "Sacrifice this" works).
    fn sac_filter_matches(&self, obj: ObjId, filter: &CardFilter, source: ObjId, payer: PlayerId) -> bool {
        match filter {
            CardFilter::ItSelf => obj == source,
            CardFilter::All(fs) => fs.iter().all(|f| self.sac_filter_matches(obj, f, source, payer)),
            CardFilter::AnyOf(fs) => fs.iter().any(|f| self.sac_filter_matches(obj, f, source, payer)),
            CardFilter::Not(f) => !self.sac_filter_matches(obj, f, source, payer),
            other => self.enter_filter_matches(obj, other, payer),
        }
    }

    /// Pay a [`CostComponent::Sacrifice`]: sacrifice `spec.min` matching permanents (CR 701.17 —
    /// move to the graveyard). When more than enough candidates exist, the payer chooses which
    /// (`SelectCards`); when exactly determined (e.g. "Sacrifice this"), no decision is asked.
    fn pay_sacrifice(&mut self, payer: PlayerId, source: ObjId, spec: &SelectSpec) {
        let candidates = self.sacrifice_candidates(payer, source, spec);
        let want = cost_count(&spec.min).min(candidates.len() as u32);
        let chosen: Vec<ObjId> = if candidates.len() as u32 <= want {
            candidates
        } else {
            let req = DecisionRequest::SelectCards {
                reason: SelectReason::Sacrifice,
                from: candidates.clone(),
                min: want,
                max: want,
                description: "sacrifice as a cost".to_string(),
            };
            let idxs = match self.ask(payer, &req) {
                DecisionResponse::Indices(i) => {
                    self.distinct_valid_indices(&i, candidates.len(), want)
                }
                _ => (0..want as usize).collect(),
            };
            idxs.into_iter().map(|i| candidates[i]).collect()
        };
        for obj in chosen {
            let owner = self.state.object(obj).owner;
            self.state.move_object(obj, Zone::Graveyard, owner);
            self.broadcast(GameEvent::ObjectMoved { obj, to: Zone::Graveyard });
        }
    }

    /// Play a land: a special action (CR 116.2a), no stack. Routed through the whiteboard so
    /// ETB replacement effects (e.g. Root Maze "lands enter tapped") apply and the ETB event
    /// fires from commit. Counts against the one-land-per-turn limit.
    pub(crate) fn play_land(&mut self, p: PlayerId, card: ObjId) {
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

    /// The warp cost a card declares via `Ability::Warp`, if any (CR 702.x).
    fn warp_cost(&self, card: ObjId) -> Option<ManaCost> {
        self.state.def_of(card).and_then(|d| {
            d.abilities.iter().find_map(|a| match a {
                Ability::Warp { cost } => Some(cost.clone()),
                _ => None,
            })
        })
    }

    /// Cast a spell from `p`'s hand (CR 601, minimal): put it on the stack (601.2a), choose
    /// targets (601.2c), auto-pay its cost (601.2f–h), and announce it cast (601.2i). `variant`
    /// selects the cost paid — the mana cost for `Normal`, the warp cost for `Warp` (CR 702.x),
    /// which also flags the spell so it's exiled at the next end step when it resolves.
    /// Affordability + target availability are pre-checked in `legal_priority_actions`, so no
    /// rewind (CR 732) is needed. The caller keeps priority with the caster (CR 601.2i).
    pub(crate) fn cast_spell(&mut self, p: PlayerId, card: ObjId, variant: CastVariant) {
        let cost = match variant {
            CastVariant::Warp => self.warp_cost(card),
            _ => self.state.object(card).chars.mana_cost.clone(),
        };
        let cost = match cost {
            Some(c) => c,
            None => return,
        };
        let effect = self.state.def_of(card).and_then(|d| d.spell_effect().cloned());

        // 601.2a: the card becomes a spell on top of the stack.
        let sid = self.state.mint_stack();
        self.move_to_stack(card, p);
        // Flag a warp-cast spell so it's exiled at the next end step when it resolves (CR 702.x).
        if variant == CastVariant::Warp {
            if let Some(o) = self.state.objects.get_mut(&card) {
                o.warp_cast = true;
            }
        }

        // 601.2b: a modal spell chooses its modes BEFORE targets — the mode determines which
        // targets exist (CR 700.2 / 601.2c). Non-modal spells choose no modes.
        let chosen_modes = match &effect {
            Some(Effect::Modal { modes, min, max, allow_repeat }) => {
                let ctx = ResolutionCtx { controller: Some(p), ..Default::default() };
                self.choose_modes(&ctx, sid, modes, *min, *max, *allow_repeat)
            }
            _ => Vec::new(),
        };
        // 601.2c targets: declared by the CHOSEN modes only (modal), else the whole effect.
        let mut specs = match &effect {
            Some(Effect::Modal { modes, .. }) => {
                let mut out = Vec::new();
                for &m in &chosen_modes {
                    if let Some(mode) = modes.get(m as usize) {
                        collect_specs_into(&mode.effect, &mut out);
                    }
                }
                out
            }
            Some(e) => collect_target_specs(e),
            None => Vec::new(),
        };
        // An Aura spell targets the permanent it will enchant (CR 601.2c / 303.4f); it has no
        // spell ability, so the target is structural. Shared with the offer-side castability gate
        // via `aura_target_spec` so the offered casts and the target decision can't drift.
        if let Some(spec) = self.aura_target_spec(card) {
            specs.push(spec);
        }

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
            modes: chosen_modes,
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
            // Defensive unhang (CR 601.2c / 728): never emit a `ChooseTargets` a seat can't answer
            // (a required slot with fewer candidates than its `min`). The offer-side
            // `card_castable_targets` gate should make this unreachable, but if candidates vanished
            // between that check and here, rewind the cast rather than deadlock. Targets are chosen
            // before costs are paid (601.2f), so nothing has been committed — the rewind just pops
            // the spell off the stack and returns the card to hand.
            if slots.iter().any(|s| (s.legal.len() as u32) < s.min) {
                self.rewind_cast(card, sid);
                return;
            }
            let req = DecisionRequest::ChooseTargets {
                for_action: ActionRef(sid),
                source: Some(card),
                slots: slots.clone(),
            };
            let resp = self.ask(p, &req);
            let chosen = parse_targets(&slots, &resp);
            let targeted = self.targeted_object_ids(&chosen);
            if let Some(obj) = self.state.stack.items.iter_mut().find(|s| s.id == sid) {
                obj.targets = chosen;
            }
            // CR 603.2: each targeted object "becomes the target" of this spell, controlled by `p`.
            self.fire_targeted(&targeted, p);
        }

        // 601.2f–h: pay the total cost (auto-tap lands), with {X} settled to `chosen_x`.
        let pay = ManaCost {
            generic: cost.generic + chosen_x * cost.x,
            colored: cost.colored.clone(),
            x: 0,
        };
        mana::auto_pay(&mut self.state, p, &pay);
        // Record the total mana spent (incl. {X}) on the spell, for an enters-with-counters-equal-
        // to-mana-spent replacement to read when it resolves (CR 601.2f–h; Dyadrine).
        let spent = pay.generic + pay.colored.values().copied().sum::<u32>();
        if let Some(o) = self.state.objects.get_mut(&card) {
            o.mana_spent = spent;
        }

        // 601.2i: the spell has been cast.
        self.broadcast(GameEvent::SpellCast {
            spell: sid,
            controller: p,
        });
    }

    /// Abort a cast in progress whose CR 601.2c target choice can't be satisfied (a required slot
    /// has no legal candidate). Targets are chosen before costs are paid (CR 601.2f), so no mana or
    /// taps have been committed — undoing the cast (CR 728, reverse the illegal action) is just
    /// popping the spell off the stack and returning the card to its owner's hand. Defensive: the
    /// offer-side `card_castable_targets` gate makes this unreachable in normal play.
    fn rewind_cast(&mut self, card: ObjId, sid: StackId) {
        self.state.stack.items.retain(|s| s.id != sid);
        let owner = self.state.object(card).owner;
        if let Some(o) = self.state.objects.get_mut(&card) {
            o.zone = Zone::Hand;
            o.controller = owner;
            o.warp_cast = false;
        }
        self.state.player_mut(owner).hand.push(card);
    }

    /// Move a card from its owner's hand onto the stack zone (the object's `ObjId` is kept;
    /// the [`StackObject`] wraps it with a `StackId`).
    fn move_to_stack(&mut self, card: ObjId, controller: PlayerId) {
        let owner = self.state.object(card).owner;
        // Remove from its current public source zone — hand, or exile for a warp recast.
        let pl = self.state.player_mut(owner);
        pl.hand.retain(|&x| x != card);
        pl.exile.retain(|&x| x != card);
        if let Some(o) = self.state.objects.get_mut(&card) {
            o.zone = Zone::Stack;
            o.controller = controller;
        }
    }

    /// CR 700.2d / 601.2c: a modal **mode** may be chosen only if every target it declares can be
    /// legally chosen. A mode that declares no targets (e.g. a search) is always legal. Used both to
    /// filter the modes offered at `choose_modes` and to decide a modal spell's castability.
    pub(crate) fn mode_is_legal(&self, mode: &crate::effects::Mode, controller: PlayerId) -> bool {
        let mut specs = Vec::new();
        collect_specs_into(&mode.effect, &mut specs);
        specs
            .iter()
            .all(|spec| self.target_candidates(spec, controller).len() as u32 >= spec.min.max(1))
    }

    /// CR 601.2c: can this spell choose legal targets, so it may be put on the stack? A normal spell
    /// needs a candidate for every target it declares. A **modal** spell instead needs at least `min`
    /// of its modes to be individually legal — modes are chosen first (601.2b), so one legal mode
    /// (e.g. an untargeted one) suffices even if another mode's targets are unavailable. This is what
    /// keeps Bushwhack castable for its search mode while you control no creatures — yet stops the
    /// engine from offering its fight mode (and a `ChooseTargets` with no legal creatures).
    pub(crate) fn spell_castable_targets(&self, effect: &Effect, p: PlayerId) -> bool {
        match effect {
            Effect::Modal { modes, min, .. } => {
                modes.iter().filter(|m| self.mode_is_legal(m, p)).count() as u32 >= *min
            }
            _ => collect_target_specs(effect)
                .iter()
                .all(|spec| self.target_candidates(spec, p).len() as u32 >= spec.min.max(1)),
        }
    }

    /// The structural target an **Aura** spell declares at CR 601.2c / 303.4f: the permanent it
    /// will enchant. Auras have no spell ability, so this target is synthesized from the card, not
    /// its effect IR — first pass, the starter set's Auras all "Enchant creature". `None` for a
    /// non-Aura. Shared by [`Engine::cast_spell`] (which builds the enchant `ChooseTargets` slot
    /// from it) and [`Engine::card_castable_targets`] (the offer-side pre-check) so the two can't
    /// drift and the engine never offers a cast it can't then satisfy.
    fn aura_target_spec(&self, card: ObjId) -> Option<TargetSpec> {
        self.is_aura(card).then_some(TargetSpec {
            kind: TargetKind::Creature(CardFilter::Any),
            min: 1,
            max: 1,
            distinct: true,
        })
    }

    /// CR 601.2c: may `card` be offered as a Cast — i.e. can it choose a legal target for **every**
    /// required slot the cast will declare? The single offer-side gate, covering BOTH the spell
    /// ability's targets (modal-aware, via [`Engine::spell_castable_targets`]) AND an Aura's
    /// structural enchant target (via [`Engine::aura_target_spec`]). `cast_spell` builds its
    /// `ChooseTargets` slots from these very specs, so the offer and the decision can't drift —
    /// this is what stops the engine offering e.g. Pacifism with no creature to enchant (which then
    /// deadlocked on a zero-candidate `ChooseTargets`).
    fn card_castable_targets(&self, card: ObjId, p: PlayerId) -> bool {
        // Spell-ability targets (modal-aware). A card with no spell ability — an Aura, or a
        // vanilla permanent — imposes no effect-side target constraint here.
        let effect_ok = match self.state.def_of(card).and_then(|d| d.spell_effect()) {
            Some(eff) => self.spell_castable_targets(eff, p),
            None => true,
        };
        if !effect_ok {
            return false;
        }
        // An Aura additionally needs a legal permanent to enchant (CR 303.4f).
        if let Some(spec) = self.aura_target_spec(card) {
            if (self.target_candidates(&spec, p).len() as u32) < spec.min.max(1) {
                return false;
            }
        }
        true
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
        // Every battlefield permanent (not just creatures) — for `TargetKind::Permanent` (e.g.
        // "target land you control"). Filtered by the spec's `CardFilter` below.
        let permanents = || {
            self.state
                .objects
                .values()
                .filter(|o| o.zone == Zone::Battlefield && self.targetable_by(o.id, caster))
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
            TargetKind::Creature(filter) => creatures()
                .filter(|t| self.target_matches_filter(t, filter, caster))
                .collect(),
            TargetKind::Permanent(filter) => permanents()
                .filter(|t| self.target_matches_filter(t, filter, caster))
                .collect(),
            // A card in a public zone — e.g. "target card from a graveyard" (Keen-Eyed Curator).
            // Enumerates every object in `zone` (any player's) matching the filter; graveyard/exile
            // are public, so no hexproof masking applies.
            TargetKind::CardInZone { zone, filter } => self
                .state
                .objects
                .values()
                .filter(|o| o.zone == *zone)
                .map(|o| Target::Object(o.id))
                .filter(|t| self.target_matches_filter(t, filter, caster))
                .collect(),
            // StackObject targeting: not needed by the current pool.
            _ => Vec::new(),
        }
    }

    /// Apply the `CardFilter` that targeting needs (the engine also pre-masks hexproof in
    /// `targetable_by`). Enforces control, card type, subtype, color and supertype so a "target
    /// land you control" can't be satisfied by a creature etc.; characteristic predicates read the
    /// COMPUTED chars (CR 613) so an animated land counts as a creature/land. Any predicate this
    /// doesn't explicitly handle **rejects** the target (fail-closed) — an unenforced filter must
    /// never silently widen the legal set into illegal targets; a card needing a new predicate adds
    /// its arm here.
    fn target_matches_filter(&self, t: &Target, filter: &CardFilter, caster: PlayerId) -> bool {
        let Target::Object(id) = t else { return true };
        let Some(o) = self.state.objects.get(id) else {
            return false;
        };
        match filter {
            CardFilter::Any => true,
            CardFilter::ControlledBy(PlayerRef::Controller | PlayerRef::Owner) => {
                o.controller == caster
            }
            CardFilter::ControlledBy(PlayerRef::Opponent | PlayerRef::EachOpponent) => {
                o.controller != caster
            }
            CardFilter::HasCardType(ct) => self.state.computed(*id).card_types.contains(ct),
            CardFilter::HasSubtype(s) => self.state.computed(*id).subtypes.contains(s),
            CardFilter::HasColor(c) => self.state.computed(*id).colors.contains(c),
            CardFilter::Colorless => self.state.computed(*id).colors.is_empty(),
            CardFilter::PowerAtMost(n) => self.state.computed(*id).power.unwrap_or(0) <= *n,
            CardFilter::Supertype(s) => o.chars.supertypes.contains(s),
            CardFilter::All(fs) => fs.iter().all(|f| self.target_matches_filter(t, f, caster)),
            CardFilter::AnyOf(fs) => fs.iter().any(|f| self.target_matches_filter(t, f, caster)),
            CardFilter::Not(f) => !self.target_matches_filter(t, f, caster),
            // Fail-closed: an unhandled predicate rejects rather than silently passing (which is
            // what let a creature match "land you control"). Add an arm above when a card needs one.
            _ => false,
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
            .is_some_and(|o| o.chars.subtypes.contains(&Subtype::Enchantment(EnchantmentType::Aura)))
    }

    /// CR 608.2b: a spell/ability resolves unless *every* target is illegal. Each target is
    /// re-checked against the zone its `TargetSpec` requires (`specs[i]`), so a graveyard target
    /// (Keen-Eyed exile) or a stack target (Surrak) isn't wrongly fizzled by a battlefield-only
    /// check (#63). `specs` is collected in the same order the targets were chosen; a missing spec
    /// falls back to the spec-less existence check. (Returns true if there are no targets.)
    fn targets_still_legal(&self, targets: &[Target], specs: &[TargetSpec]) -> bool {
        if targets.is_empty() {
            return true;
        }
        targets.iter().enumerate().any(|(i, t)| match specs.get(i) {
            Some(spec) => self.target_legal_for(t, spec),
            None => self.target_legal(t),
        })
    }

    /// A chosen target re-checked against the zone its spec requires (CR 608.2b) — the #63 fix.
    /// A `CardInZone` target must still be in that public zone; a `StackObject` target still on the
    /// stack; everything else (creature/permanent/any) still on the battlefield (so a battlefield
    /// target that died — moved to the graveyard — correctly becomes illegal).
    fn target_legal_for(&self, t: &Target, spec: &TargetSpec) -> bool {
        match t {
            Target::Player(p) => {
                self.state.players.get(p.0 as usize).is_some_and(|pl| !pl.has_lost)
            }
            Target::Stack(sid) => self.state.stack.items.iter().any(|s| s.id == *sid),
            Target::Object(o) => self.state.objects.get(o).is_some_and(|obj| match &spec.kind {
                TargetKind::CardInZone { zone, .. } => obj.zone == *zone,
                TargetKind::StackObject(_) => obj.zone == Zone::Stack,
                _ => obj.zone == Zone::Battlefield,
            }),
        }
    }

    /// The spec-less fallback: whether `t` still exists on the battlefield / as a live player. Used
    /// only when a caller has no `TargetSpec` to hand; spec-aware callers use [`target_legal_for`].
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

    /// Snapshot the controller of each chosen target at resolution start (parallel to `targets`;
    /// `None` for non-object targets). Lets `PlayerRef::ControllerOfTarget` resolve to a target's
    /// controller even after that object leaves play during the same resolution (CR 608.2) — e.g.
    /// Erode's "Destroy target creature. Its controller may search…".
    fn snapshot_target_controllers(&self, targets: &[Target]) -> Vec<Option<PlayerId>> {
        targets
            .iter()
            .map(|t| match t {
                Target::Object(id) => self.state.objects.get(id).map(|o| o.controller),
                _ => None,
            })
            .collect()
    }

    /// Resolve the top object of the stack (CR 608). Milestone 2 performs only the
    /// *structural* part — a permanent spell enters the battlefield, an instant/sorcery
    /// goes to its owner's graveyard, an ability ceases to exist (608.2n/608.3). Running
    /// the object's effect IR is the effect runtime's job (milestone 4). In a lands-only
    /// game the stack stays empty, so this is exercised only by unit tests.
    pub(crate) fn resolve_top(&mut self) {
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
                    if self.is_aura(id) && !self.targets_still_legal(&obj.targets, &[]) {
                        self.state.move_object(id, Zone::Graveyard, owner);
                        self.broadcast(GameEvent::ObjectMoved { obj: id, to: Zone::Graveyard });
                        return;
                    }
                    // Was it cast for its warp cost? (read before commit resets the flag.)
                    let warp_cast = self.state.object(id).warp_cast;
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
                    // Warp (CR 702.x): arm "exile this at the beginning of the next end step" as a
                    // delayed trigger (CR 603.7) watching the permanent that just entered.
                    if warp_cast {
                        self.state.register_delayed_trigger(
                            id,
                            crate::effects::action::DelayedTriggerEvent::AtBeginningOfNextEndStep,
                            obj.controller,
                            Some(id),
                            vec![Action::WarpExile { obj: id }],
                        );
                    }
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
                        if self.targets_still_legal(&obj.targets, &target_specs_for(&effect, &obj.modes)) {
                            let ctx = ResolutionCtx {
                                controller: Some(obj.controller),
                                source: Some(id),
                                x: obj.x,
                                target_controllers: self.snapshot_target_controllers(&obj.targets),
                                chosen_targets: obj.targets.clone(),
                                chosen_modes: obj.modes.clone(),
                                ability_index: None,
                                triggering_spell: None,
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
                    if self.targets_still_legal(&obj.targets, &target_specs_for(&effect, &obj.modes)) {
                        let ctx = ResolutionCtx {
                            controller: Some(obj.controller),
                            source: Some(src),
                            x: None,
                            target_controllers: self.snapshot_target_controllers(&obj.targets),
                            chosen_targets: obj.targets.clone(),
                            chosen_modes: Vec::new(),
                            // So a reflexive "when you do" branch can reference back into this ability.
                            ability_index: Some(index),
                            // A "whenever you cast …" trigger carries its triggering spell (Opus).
                            triggering_spell: self.state.trigger_source_spell.get(&obj.id).copied(),
                        };
                        self.resolve_effect(&effect, &ctx, WbReason::Resolve(obj.id));
                    }
                }
                // The triggering-spell association is consumed once the trigger resolves.
                self.state.trigger_source_spell.remove(&obj.id);
            }
            StackObjectKind::ReflexiveAbility { source, ability_index } => {
                // A reflexive "when you do" sub-trigger (CR 603.7c): re-check the intervening-if
                // (603.4, now that the parent's actions committed) and, if it still holds, resolve
                // the reward (`then`) with the targets chosen as it went on the stack. Resolving
                // `then` directly (not the Conditional node) avoids re-deferring it.
                let node = self.reflexive_node(source, ability_index);
                let then = node.as_ref().and_then(|n| reflexive_reward(&self.state, n, source)).cloned();
                if let Some(then) = then {
                    if self.targets_still_legal(&obj.targets, &target_specs_for(&then, &[])) {
                        let ctx = ResolutionCtx {
                            controller: Some(obj.controller),
                            source: Some(source),
                            target_controllers: self.snapshot_target_controllers(&obj.targets),
                            chosen_targets: obj.targets.clone(),
                            ability_index: Some(ability_index),
                            ..Default::default()
                        };
                        self.resolve_effect(&then, &ctx, WbReason::Resolve(obj.id));
                    }
                }
            }
            StackObjectKind::DelayedAbility { ref actions } => {
                // A fired delayed triggered ability (CR 603.7): commit its concrete actions
                // directly (no `Effect` tree / no targets). E.g. Earthbend's return-it-tapped.
                let actions = actions.clone();
                let ctx = ResolutionCtx {
                    controller: Some(obj.controller),
                    source: obj.source,
                    ..Default::default()
                };
                let mut wb = Whiteboard::new(WbReason::Resolve(obj.id), ctx);
                for a in actions {
                    wb.push(a);
                }
                self.commit(wb);
            }
        }
    }

    // ── the agenda pipeline (CR 117.5) ────────────────────────────────────────────────────

    /// Run the agenda to a fixpoint: recompute continuous effects → perform SBAs (loop
    /// until none) → put waiting triggers on the stack (APNAP) → repeat until stable.
    /// This is the law from WHITEBOARD_MODEL §2.2; run before any player receives priority.
    /// Safety ceiling for an unbounded engine fixpoint loop (#55). Once a single loop has spun
    /// `limit` times without the game ending, it is wedged (an infinite loop bug): abort the game to
    /// a DRAW and log once, naming the loop, so self-play/training can NEVER hang. Returns `true`
    /// when tripped so the caller bails out of its loop. `pub(crate)` so the resolution-side fixpoints
    /// in `whiteboard.rs` (replacement/prevention rewrite) share the same guard.
    pub(crate) fn loop_guard_tripped(&mut self, iters: usize, limit: usize, loop_name: &str) -> bool {
        if iters < limit {
            return false;
        }
        if !self.state.game_over {
            eprintln!(
                "mtgenv: engine loop-guard tripped in {loop_name} after {limit} iterations \
                 (turn {}, phase {:?}, stack {}) — aborting game to a draw; this is an engine \
                 infinite-loop bug.",
                self.state.turn_number,
                self.state.phase,
                self.state.stack.items.len(),
            );
            self.state.game_over = true;
            self.state.winner = None;
        }
        true
    }

    /// Test/audit helper: drive to an empty stack — stabilize (SBAs + queue pending triggers via
    /// [`run_agenda`]) then resolve the top, repeating until the stack is empty (or the game ends).
    /// Lets a trigger test be a one-liner after `play_land`/`cast_spell`/`declare_attackers_explicit`.
    #[allow(dead_code)] // test/audit-harness helper (in-crate tests only)
    pub(crate) fn resolve_to_stable(&mut self) {
        let mut iters = 0usize;
        loop {
            if self.loop_guard_tripped(iters, PRIORITY_LOOP_LIMIT, "resolve_to_stable") {
                break;
            }
            iters += 1;
            self.run_agenda();
            if self.state.game_over || self.state.stack.is_empty() {
                break;
            }
            self.resolve_top();
        }
    }

    /// Drive the game to a stable state (CR 704/603.3): perform state-based actions and put any
    /// waiting triggered abilities on the stack, repeating to a fixpoint. `pub(crate)` so in-crate
    /// tests (and design's #60 end-to-end card audit) can drain the triggers/SBAs a `cast_spell` /
    /// `play_land` / `resolve_top` spawns — e.g. a land-drop's landfall, an ETB, an attack trigger —
    /// which `resolve_top` alone does NOT process. NB: it only *queues* the triggers onto the stack;
    /// call `resolve_top` to resolve each.
    pub(crate) fn run_agenda(&mut self) {
        let mut iters = 0usize;
        loop {
            if self.loop_guard_tripped(iters, AGENDA_LOOP_LIMIT, "run_agenda (SBA/trigger fixpoint)") {
                return;
            }
            iters += 1;
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
    /// The cloned reflexive `Conditional` node of `source`'s `ability_index` ability, if any.
    fn reflexive_node(&self, source: ObjId, ability_index: u32) -> Option<Effect> {
        self.state.def_of(source).and_then(|d| {
            d.abilities
                .get(ability_index as usize)
                .and_then(|a| match a {
                    Ability::Triggered { effect, .. } | Ability::Activated { effect, .. } => {
                        Some(effect)
                    }
                    _ => None,
                })
                .and_then(reflexive_branch)
                .cloned()
        })
    }

    fn put_trigger_on_stack(&mut self, mut t: StackObject) {
        let effect = match (t.source, &t.kind) {
            (Some(src), StackObjectKind::Ability { index }) => {
                self.state.def_of(src).and_then(|d| match d.abilities.get(*index as usize) {
                    Some(Ability::Triggered { effect, .. }) => Some(effect.clone()),
                    _ => None,
                })
            }
            // A reflexive sub-trigger chooses the targets of its reward (CR 603.7c/603.3d) — but
            // only when its intervening-if holds (603.4, re-checked now the parent has committed).
            (_, StackObjectKind::ReflexiveAbility { source, ability_index }) => self
                .reflexive_node(*source, *ability_index)
                .as_ref()
                .and_then(|n| reflexive_reward(&self.state, n, *source))
                .cloned(),
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
                    source: t.source, // the triggering / reflexive permanent (not yet on the stack)
                    slots: slots.clone(),
                };
                let resp = self.ask(t.controller, &req);
                t.targets = parse_targets(&slots, &resp);
            }
        }
        let targeted = self.targeted_object_ids(&t.targets);
        let by = t.controller;
        self.state.stack.push(t);
        // CR 603.2: each targeted object becomes the target of this triggered ability.
        self.fire_targeted(&targeted, by);
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
        // Invariant (CR 601.2c / 728): every decision the engine emits must have ≥1 legal response,
        // else the game deadlocks. Debug-only canary — the offer-side gates + cast rewind uphold it.
        debug_assert!(
            request_has_legal_response(req),
            "engine emitted a DecisionRequest with no legal response (deadlock): {req:?}"
        );
        let mut view = self.view_for_seat(p);
        self.reveal_request_objects(&mut view, req);
        // The one decision seam (RESUMABLE_ENGINE.md §3.2): inside a `Session` fiber, SUSPEND and
        // hand the decision to the driver; on the blocking path, call the in-core agent directly.
        match self.yielder {
            Some(y) => {
                // SAFETY: `y` points at this fiber's `Yielder`, which is live for the whole fiber
                // body; `ask` only runs while the fiber is running, so the pointer is never dangling.
                let step = crate::session::Step::Decision { seat: p, view, request: req.clone() };
                unsafe { (*y).suspend(step) }
            }
            None => self.agents[p.0 as usize].decide(&view, req),
        }
    }

    /// Surface, in the deciding seat's view, the characteristics of any objects the
    /// `DecisionRequest` references that aren't already perceivable — chiefly a Search /
    /// `SelectCards` whose candidates are drawn from the hidden library. The seat is choosing among
    /// those exact cards, so it's entitled to see them (the rest of the hidden zone stays masked);
    /// without this they render as bare ids (#43). Added to `me.revealed_to_me` (self-only, rebuilt
    /// per decision). The general invariant: the view handed to `decide()` describes every object
    /// the request names.
    fn reveal_request_objects(&self, view: &mut PlayerView, req: &DecisionRequest) {
        let referenced = request_object_ids(req);
        if referenced.is_empty() {
            return;
        }
        // Ids already perceivable in the view, so we don't duplicate on-board/in-hand/public cards.
        let id_of = |v: &ObjView| match v {
            ObjView::Visible { id, .. } | ObjView::Hidden { id, .. } => *id,
        };
        let mut seen: std::collections::BTreeSet<ObjId> = std::collections::BTreeSet::new();
        seen.extend(view.battlefield.iter().map(id_of));
        seen.extend(view.me.hand.iter().map(id_of));
        seen.extend(view.me.known_library.iter().map(id_of));
        seen.extend(view.me.revealed_to_me.iter().map(id_of));
        for pv in &view.players {
            seen.extend(pv.graveyard.iter().map(id_of));
            seen.extend(pv.exile_public.iter().map(id_of));
        }
        let missing: Vec<ObjId> = referenced.into_iter().filter(|id| !seen.contains(id)).collect();
        if !missing.is_empty() {
            view.me
                .revealed_to_me
                .extend(crate::state::view::reveal_objects(&self.state, &missing));
        }
    }

    /// The info-filtered [`PlayerView`] for `p`, augmented with the seat's stop state (the
    /// settings-echo — `PlayerView.stops`; `None` under full control, where every window stops so
    /// there's nothing to elide/echo). Used everywhere the engine builds a view (decide/observe).
    fn view_for_seat(&self, p: PlayerId) -> PlayerView {
        let mut view = view_for(&self.state, p);
        // Snapshot the config once (don't hold the lock while computing per-step, which would
        // re-enter the same non-reentrant Mutex via `stops_at`).
        let cfg = self.stops[p.0 as usize].lock().unwrap().clone();
        if !cfg.full_control {
            let active = self.state.active_player;
            let per_step = TURN_STEPS
                .iter()
                .copied()
                .filter(|&s| step_grants_priority(s))
                .map(|s| (s, cfg.stops_at(p, s, active)))
                .collect();
            view.stops = Some(StopStateView { full_control: cfg.full_control, per_step });
        }
        view
    }

    /// Push a public event to every seat's `observe` channel (CR: the GRE diff stream), and
    /// collect any triggered abilities that watch this event (CR 603.2).
    pub(crate) fn broadcast(&mut self, ev: GameEvent) {
        // `ManaPoolChanged` is a live-view-only refresh (#59/#62): observers see floating mana
        // mid-resolution, but it isn't recorded — it would bloat the event log / replay with churn.
        let record = !matches!(ev, GameEvent::ManaPoolChanged { .. });
        if record && self.record_events {
            self.event_log.push(ev.clone());
        }
        // Capture an omniscient replay frame of the post-event state, labelled by the event.
        if record && self.record_replay {
            let label = self.event_label(&ev);
            self.push_replay_frame(label);
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
                // Delayed "when this dies …" abilities (CR 603.7) fire on death.
                self.fire_delayed_triggers(*obj);
            }
            // "Whenever you gain life" (CR 603.2) — a positive life change fires each GainLife
            // trigger on a permanent the gaining player controls, once per gain event.
            GameEvent::LifeChanged { player, delta, .. } if *delta > 0 => {
                let watchers: Vec<ObjId> = self.state.player(*player).battlefield.clone();
                for w in watchers {
                    self.queue_self_triggers(w, EventPattern::GainLife);
                }
            }
            // Delayed "when this … is exiled" abilities fire when the watched object reaches exile.
            GameEvent::ObjectMoved { obj, to: Zone::Exile } => {
                self.fire_delayed_triggers(*obj);
            }
            // "Whenever [filter] becomes the target of a spell/ability …" (CR 603.2). C16.
            GameEvent::Targeted { object, by } => {
                self.queue_watching_targeted_triggers(*object, *by);
            }
            // Attackers declared (CR 508.1): per-attacker "this attacks" + once "you attack".
            GameEvent::AttackersDeclared { attackers, by } => {
                for &atk in attackers {
                    self.queue_self_triggers(atk, EventPattern::SelfAttacks);
                }
                self.queue_you_attack_triggers(*by);
            }
            // Beginning of the end step (CR 513): fire armed "at the next end step" delayed triggers
            // (warp's exile-at-end-step). Sorcery-speed-armed ⇒ "next" = this turn's end step.
            GameEvent::PhaseBegan { phase: Phase::End, .. } => {
                self.fire_end_step_delayed_triggers();
            }
            // "Whenever you cast a [filter] spell" (CR 603.2 / 601.2i) — SoS Opus / Repartee /
            // Increment. Queue each matching `SpellCast` trigger on a permanent the *caster*
            // controls, recording the triggering spell so the ability can read its mana-spent.
            GameEvent::SpellCast { spell, controller } => {
                self.queue_watching_spellcast_triggers(*spell, *controller);
            }
            _ => {}
        }
    }

    /// Fire (and consume, CR 603.7b) every armed `AtBeginningOfNextEndStep` delayed trigger — the
    /// warp exile-at-end-step clause. Queued onto the stack like any trigger.
    fn fire_end_step_delayed_triggers(&mut self) {
        use crate::effects::action::DelayedTriggerEvent;
        let mut fired = Vec::new();
        self.state.delayed_triggers.retain(|dt| {
            let is_step = matches!(dt.event, DelayedTriggerEvent::AtBeginningOfNextEndStep);
            if is_step {
                fired.push(dt.clone());
            }
            !is_step
        });
        for dt in fired {
            let id = self.state.mint_stack();
            self.state.pending_triggers.push(StackObject {
                id,
                controller: dt.controller,
                source: dt.source,
                kind: StackObjectKind::DelayedAbility { actions: dt.actions },
                targets: Vec::new(),
                x: None,
                modes: Vec::new(),
            });
        }
    }

    /// Queue every battlefield permanent controlled by `attacker` that has a `YouAttack` trigger —
    /// "whenever you attack …", fired once per combat for the attacking player (CR 508.1).
    fn queue_you_attack_triggers(&mut self, attacker: PlayerId) {
        let watchers: Vec<ObjId> = self.state.player(attacker).battlefield.clone();
        for watcher in watchers {
            let indices: Vec<u32> = match self.state.def_of(watcher) {
                Some(def) => def
                    .abilities
                    .iter()
                    .enumerate()
                    .filter(|(_, a)| {
                        matches!(a, Ability::Triggered { event: EventPattern::YouAttack, .. })
                    })
                    .map(|(i, _)| i as u32)
                    .collect(),
                None => continue,
            };
            for index in indices {
                let id = self.state.mint_stack();
                self.state.pending_triggers.push(StackObject {
                    id,
                    controller: attacker,
                    source: Some(watcher),
                    kind: StackObjectKind::Ability { index },
                    targets: Vec::new(),
                    x: None,
                    modes: Vec::new(),
                });
            }
        }
    }

    /// The object ids that "become the target" among a set of chosen `Target`s (CR 603.2): a direct
    /// `Object` target, or — for a spell on the stack — its underlying card object, so "a creature
    /// **spell** you control becomes the target" fires too (Surrak's stack-half). Players excluded.
    fn targeted_object_ids(&self, targets: &[Target]) -> Vec<ObjId> {
        targets
            .iter()
            .filter_map(|t| match t {
                Target::Object(id) => Some(*id),
                Target::Stack(sid) => self
                    .state
                    .stack
                    .items
                    .iter()
                    .find(|s| s.id == *sid)
                    .and_then(|s| match s.kind {
                        StackObjectKind::Spell(obj) => Some(obj),
                        _ => None,
                    }),
                Target::Player(_) => None,
            })
            .collect()
    }

    /// Broadcast a `Targeted` event for each object that became a target (CR 603.2), controlled by
    /// `by` — drives "becomes the target of a spell or ability" triggers.
    fn fire_targeted(&mut self, objects: &[ObjId], by: PlayerId) {
        for &object in objects {
            self.broadcast(GameEvent::Targeted { object, by });
        }
    }

    /// Fire (and consume, CR 603.7b) every armed delayed triggered ability watching `obj` whose
    /// event matches its leaving the battlefield. Each fired ability is queued onto the stack
    /// carrying its concrete actions; it then resolves through the normal agenda like any trigger.
    fn fire_delayed_triggers(&mut self, obj: ObjId) {
        let mut fired = Vec::new();
        self.state.delayed_triggers.retain(|dt| {
            let matches = dt.watching == obj
                && matches!(dt.event, crate::effects::action::DelayedTriggerEvent::DiesOrExiled);
            if matches {
                fired.push(dt.clone());
            }
            !matches
        });
        for dt in fired {
            let id = self.state.mint_stack();
            self.state.pending_triggers.push(StackObject {
                id,
                controller: dt.controller,
                source: dt.source,
                kind: StackObjectKind::DelayedAbility { actions: dt.actions },
                targets: Vec::new(),
                x: None,
                modes: Vec::new(),
            });
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
                modes: Vec::new(),
            });
        }
    }

    /// Queue every battlefield permanent's `PermanentEnters(filter)` trigger that matches the
    /// just-entered object (CR 603.2). The filter is evaluated relative to the WATCHER's
    /// controller, so "a land you control enters" (landfall) means the watcher's controller.
    /// Queue "whenever you cast a [filter] spell" triggers (CR 603.2) for the caster's permanents,
    /// recording each trigger's [`StackId`] → the triggering spell's card so the ability can read
    /// its mana-spent at resolution (SoS Opus / Repartee / Increment). Only the *caster's* permanents
    /// watch (the "you" in "whenever you cast …").
    fn queue_watching_spellcast_triggers(&mut self, spell: StackId, caster: PlayerId) {
        let (card, targets) = match self.state.stack.items.iter().find(|s| s.id == spell) {
            Some(s) => match s.kind {
                StackObjectKind::Spell(o) => (o, s.targets.clone()),
                _ => return,
            },
            None => return,
        };
        // "targets a creature" (CR 603.2, Repartee): any chosen object target that's a creature.
        let targets_a_creature = targets.iter().any(|t| {
            matches!(t, Target::Object(o) if self.state.computed(*o).card_types.contains(&CardType::Creature))
        });
        let watchers: Vec<ObjId> = self.state.player(caster).battlefield.clone();
        for watcher in watchers {
            // (index, filter, needs_creature_target)
            let candidates: Vec<(u32, CardFilter, bool)> = match self.state.def_of(watcher) {
                Some(def) => def
                    .abilities
                    .iter()
                    .enumerate()
                    .filter_map(|(i, a)| match a {
                        Ability::Triggered { event: EventPattern::SpellCast(f), .. } => {
                            Some((i as u32, f.clone(), false))
                        }
                        Ability::Triggered {
                            event: EventPattern::SpellCastTargetingCreature(f), ..
                        } => Some((i as u32, f.clone(), true)),
                        _ => None,
                    })
                    .collect(),
                None => continue,
            };
            for (index, filter, needs_creature_target) in candidates {
                if (!needs_creature_target || targets_a_creature)
                    && self.enter_filter_matches(card, &filter, caster)
                {
                    let id = self.state.mint_stack();
                    self.state.pending_triggers.push(StackObject {
                        id,
                        controller: caster,
                        source: Some(watcher),
                        kind: StackObjectKind::Ability { index },
                        targets: Vec::new(),
                        x: None,
                        modes: Vec::new(),
                    });
                    self.state.trigger_source_spell.insert(id, card);
                }
            }
        }
    }

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
                        modes: Vec::new(),
                    });
                }
            }
        }
    }

    /// Queue every battlefield permanent's `BecomesTargeted` trigger that matches `object` having
    /// just become the target of a spell/ability controlled by `by` (CR 603.2). The `filter` is
    /// evaluated relative to the WATCHER's controller ("a creature you control"); `by_opponent`
    /// requires the targeting source to be controlled by an opponent of the watcher (C16, Surrak).
    fn queue_watching_targeted_triggers(&mut self, object: ObjId, by: PlayerId) {
        let watchers: Vec<ObjId> = self
            .state
            .players
            .iter()
            .flat_map(|p| p.battlefield.iter().copied())
            .collect();
        for watcher in watchers {
            let wctrl = self.state.object(watcher).controller;
            let candidates: Vec<(u32, CardFilter, bool)> = match self.state.def_of(watcher) {
                Some(def) => def
                    .abilities
                    .iter()
                    .enumerate()
                    .filter_map(|(i, a)| match a {
                        Ability::Triggered {
                            event: EventPattern::BecomesTargeted { filter, by_opponent },
                            ..
                        } => Some((i as u32, filter.clone(), *by_opponent)),
                        _ => None,
                    })
                    .collect(),
                None => continue,
            };
            for (index, filter, by_opponent) in candidates {
                // The targeting source must be an opponent of the watcher (2-player: `by != wctrl`).
                if by_opponent && by == wctrl {
                    continue;
                }
                if self.enter_filter_matches(object, &filter, wctrl) {
                    let id = self.state.mint_stack();
                    self.state.pending_triggers.push(StackObject {
                        id,
                        controller: wctrl,
                        source: Some(watcher),
                        kind: StackObjectKind::Ability { index },
                        targets: Vec::new(),
                        x: None,
                        modes: Vec::new(),
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

/// Evaluate a fixed cost count (e.g. a sacrifice count, CR 118). The current pool's cost counts
/// are constants; a non-`Fixed` expr defaults to 1 (revisit when a variable cost actually arrives).
fn cost_count(v: &ValueExpr) -> u32 {
    match v {
        ValueExpr::Fixed(n) => (*n).max(0) as u32,
        _ => 1,
    }
}

/// Collect the `TargetSpec`s an `Effect` requires, in declaration order (CR 601.2c). The
/// milestone-3 starter set only needs the `DealDamage` target; `Sequence` recurses. Other
/// targeted IR nodes are added as their cards arrive.
pub(crate) fn collect_target_specs(effect: &Effect) -> Vec<TargetSpec> {
    let mut out = Vec::new();
    collect_specs_into(effect, &mut out);
    out
}

/// The `TargetSpec`s an effect's targets were chosen against, in the SAME order they were collected
/// at cast/activation — so they align 1:1 with a stack object's `targets` for the resolution-time
/// re-check (CR 608.2b, #63). For a modal effect this is the specs of the CHOSEN modes only (a
/// non-chosen mode contributes no targets); otherwise the whole effect's specs.
pub(crate) fn target_specs_for(effect: &Effect, chosen_modes: &[u32]) -> Vec<TargetSpec> {
    match effect {
        Effect::Modal { modes, .. } => {
            let mut out = Vec::new();
            for &m in chosen_modes {
                if let Some(mode) = modes.get(m as usize) {
                    collect_specs_into(&mode.effect, &mut out);
                }
            }
            out
        }
        e => collect_target_specs(e),
    }
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
        }
        | Effect::PumpPT {
            what: EffectTarget::Target(spec),
            ..
        } => out.push(spec.clone()),
        // A fight declares two targets (CR 701.12) — each `Target(spec)` is a chosen target.
        Effect::Fight { a, b } => {
            for t in [a, b] {
                if let EffectTarget::Target(spec) = t {
                    out.push(spec.clone());
                }
            }
        }
        // "Target player" (CR 115.1) — a targeting slot the following effects reference via
        // `PlayerRef::ChosenTarget`. Declares a single Player target.
        Effect::TargetPlayer => out.push(TargetSpec {
            kind: TargetKind::Player,
            min: 1,
            max: 1,
            distinct: true,
        }),
        // Earthbend targets "target land you control" (CR 601.2c).
        Effect::Earthbend { target: EffectTarget::Target(spec), .. } => out.push(spec.clone()),
        // Exile targets "target card from a graveyard" etc. (CR 601.2c).
        Effect::Exile { what: EffectTarget::Target(spec) } => out.push(spec.clone()),
        // "Put a +1/+1 counter on target creature" / "target creature gains trample" — the targeted
        // reward effects (collected when walking a reflexive branch, not from a Conditional.then).
        Effect::PutCounters { what: EffectTarget::Target(spec), .. }
        | Effect::GrantKeyword { what: EffectTarget::Target(spec), .. }
        | Effect::GrantQualification { what: EffectTarget::Target(spec), .. }
        | Effect::Tap { what: EffectTarget::Target(spec), .. } => out.push(spec.clone()),
        Effect::Sequence(effects) => {
            for e in effects {
                collect_specs_into(e, out);
            }
        }
        // NOTE: `Conditional`/`Optional` are NOT walked — a target inside them is a reflexive
        // trigger (CR 603.7c) whose target is chosen on the sub-trigger, not the parent.
        _ => {}
    }
}

/// The reflexive "when you do" branch of an ability's effect (CR 603.7c): the first
/// `Conditional` node whose `then` is targeted, in a pre-order walk. Returns the **Conditional
/// node** (so its `cond` is evaluated when the reflexive sub-trigger goes on the stack / resolves —
/// after the parent's actions commit — and its `then` supplies the deferred target). `None` if the
/// ability has no reflexive (targeted-deferred) branch.
pub(crate) fn reflexive_branch(effect: &Effect) -> Option<&Effect> {
    match effect {
        Effect::Sequence(effects) => effects.iter().find_map(reflexive_branch),
        Effect::Conditional { then, otherwise, .. } => {
            if !collect_target_specs(then).is_empty() {
                Some(effect)
            } else {
                reflexive_branch(then).or_else(|| otherwise.as_deref().and_then(reflexive_branch))
            }
        }
        _ => None,
    }
}

/// Whether a reflexive `Conditional` node's intervening-if currently holds, evaluated relative to
/// `source` (CR 603.4) — and its `then` reward. Returns `None` if the condition is false.
fn reflexive_reward<'a>(
    state: &'a GameState,
    node: &'a Effect,
    source: ObjId,
) -> Option<&'a Effect> {
    match node {
        Effect::Conditional { cond, then, .. } => {
            let controller = state.objects.get(&source).map(|o| o.controller)?;
            crate::conditions::holds_for_source(state, cond, controller, Some(source))
                .then_some(then.as_ref())
        }
        _ => None,
    }
}


/// The object ids a `DecisionRequest` names as choices/recipients — so the deciding seat's view can
/// describe them (CR-wise: the player is entitled to see what they're choosing among). Covers the
/// requests that can reference cards in otherwise-hidden zones (Search/`SelectCards`, ordering,
/// arranging) plus object targets/recipients; other requests reference nothing extra.
fn request_object_ids(req: &DecisionRequest) -> Vec<ObjId> {
    let from_targets = |ts: &[Target]| -> Vec<ObjId> {
        ts.iter()
            .filter_map(|t| match t {
                Target::Object(id) => Some(*id),
                _ => None,
            })
            .collect()
    };
    match req {
        DecisionRequest::SelectCards { from, .. } => from.clone(),
        DecisionRequest::SelectFromGroups { groups, .. } => {
            groups.iter().flat_map(|g| g.options.iter().copied()).collect()
        }
        DecisionRequest::ArrangeCards { cards, .. } => cards.clone(),
        DecisionRequest::OrderObjects { items, .. } => items.clone(),
        DecisionRequest::ChooseTargets { slots, .. } => {
            slots.iter().flat_map(|s| from_targets(&s.legal)).collect()
        }
        DecisionRequest::Distribute { among, .. } => from_targets(among),
        _ => Vec::new(),
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

/// Whether `req` provably admits at least one legal response. The engine must never emit a decision
/// a seat can't answer — that deadlocks the game (CR 601.2c / 728). The only requests that can be
/// unsatisfiable are the "choose ≥`min` from a finite candidate set" kinds; every other request
/// (Priority-with-pass, Declare{Attackers,Blockers} where declaring nothing is legal, Confirm,
/// ChooseNumber with min≤max, …) always has one. Used as a debug-only invariant at the [`Engine::ask`]
/// seam — a canary that catches any future path that would emit an unanswerable decision.
fn request_has_legal_response(req: &DecisionRequest) -> bool {
    match req {
        DecisionRequest::ChooseTargets { slots, .. } => {
            slots.iter().all(|s| s.legal.len() as u32 >= s.min)
        }
        DecisionRequest::SelectCards { from, min, .. } => from.len() as u32 >= *min,
        DecisionRequest::ChooseModes { modes, min, .. } => modes.len() as u32 >= *min,
        DecisionRequest::ChooseOption { options, min, .. } => options.len() as u32 >= *min,
        // Other requests always admit a legal response (empty declaration, pass, a number in
        // range, a confirm, …) — not a hang vector.
        _ => true,
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

    #[test]
    fn record_replay_captures_omniscient_frames() {
        use crate::replay::{Replay, ReplaySource};
        let mut e = lands_only_game(12, 7);
        e.record_replay(true);
        e.set_replay_source(ReplaySource::AiTraining { step: 42 });
        let _ = e.run_game();
        let replay = e.replay();

        // An initial "game start" frame plus one per public event → a frame stream.
        assert!(replay.frames.len() > 5, "frame stream ({} frames)", replay.frames.len());
        assert_eq!(replay.frames[0].label, "game start");
        assert_eq!(replay.frames[0].state.players.len(), 2, "both seats present");

        // God view = NO hidden info: the library (which PlayerView masks to a count) is visible.
        let saw_library = replay
            .frames
            .iter()
            .any(|f| f.state.players.iter().any(|p| !p.library.is_empty()));
        assert!(saw_library, "libraries are visible in the omniscient view");

        // Caller-set source survived; result is filled now the game is over; clock left to caller.
        assert_eq!(replay.meta.source, ReplaySource::AiTraining { step: 42 });
        assert!(replay.meta.result.is_some(), "result filled once game_over");
        assert_eq!(replay.meta.created_at, 0, "created_at left for the caller to stamp");

        // The whole replay round-trips through JSON — the shared wire contract webui/gym consume.
        let json = serde_json::to_string(&replay).unwrap();
        let back: Replay = serde_json::from_str(&json).unwrap();
        assert_eq!(back.frames.len(), replay.frames.len());
        assert_eq!(back.meta.source, ReplaySource::AiTraining { step: 42 });
    }

    #[test]
    fn replay_sink_streams_frames_live() {
        use std::cell::RefCell;
        use std::rc::Rc;
        // A live spectator forwards each frame as it's captured; here we just count + keep labels.
        let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let sink_seen = seen.clone();
        let mut e = lands_only_game(12, 9);
        e.set_replay_sink(Box::new(move |frame| {
            sink_seen.borrow_mut().push(frame.label.clone());
        }));
        let _ = e.run_game();

        let streamed = seen.borrow();
        // The sink saw the "game start" frame first, then one per event — same count the engine kept.
        assert_eq!(streamed.len(), e.replay_frame_count(), "sink saw every captured frame");
        assert!(streamed.len() > 5);
        assert_eq!(streamed[0], "game start");
        assert!(streamed.last().unwrap().starts_with("Game over"), "last frame is the game end");
    }

    #[test]
    fn controller_of_target_survives_the_destroy() {
        // Erode-shaped (CR 608.2): `Sequence[ Destroy(target 0), GainLife(ControllerOfTarget(0)) ]`.
        // "Its controller" = the destroyed creature's controller, read at resolution start, so it
        // survives the destroy moving the creature to the graveyard.
        use crate::basics::{CardType, Target};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::{Effect, EffectTarget};

        let mut e = lands_only_game(0, 1);
        // A vanilla creature controlled (and owned) by P1 — the "its controller" Erode references.
        let victim = e.state.add_card(
            PlayerId(1),
            Characteristics {
                name: "Victim".into(),
                card_types: vec![CardType::Creature],
                power: Some(2),
                toughness: Some(2),
                grp_id: 9000,
                ..Default::default()
            },
            Zone::Battlefield,
        );
        let p1_before = e.state.player(PlayerId(1)).life;
        let p0_before = e.state.player(PlayerId(0)).life;

        let effect = Effect::Sequence(vec![
            Effect::Destroy { what: EffectTarget::ChosenIndex(0) },
            Effect::GainLife { who: PlayerRef::ControllerOfTarget(0), amount: ValueExpr::Fixed(3) },
        ]);
        // P0 is the spell's controller; target 0 is P1's creature. The controller is snapshotted
        // (as `resolve_top` does) while the creature is still in play.
        let targets = vec![Target::Object(victim)];
        let ctx = ResolutionCtx {
            controller: Some(PlayerId(0)),
            target_controllers: e.snapshot_target_controllers(&targets),
            chosen_targets: targets,
            ..Default::default()
        };
        e.resolve_effect(&effect, &ctx, WbReason::Resolve(StackId(0)));

        assert_eq!(e.state.object(victim).zone, Zone::Graveyard, "target creature destroyed");
        // The *destroyed creature's* controller (P1) gained the life — not the spell's controller (P0).
        assert_eq!(e.state.player(PlayerId(1)).life, p1_before + 3, "its controller (P1) gained 3");
        assert_eq!(e.state.player(PlayerId(0)).life, p0_before, "the caster (P0) did not");
    }

    #[test]
    fn sacrifice_cost_sacrifices_the_source() {
        // CR 118/701.17: a "{T}, Sacrifice this:" activation cost. Paying it moves the source to
        // the graveyard (the unblocker for fetch lands like Fabled Passage / Escape Tunnel).
        use crate::effects::ability::{Cost, CostComponent};
        use crate::effects::target::{CardFilter, SelectSpec};
        use crate::effects::value::{PlayerRef, ValueExpr};

        let mut e = lands_only_game(5, 1);
        let src = e.state.add_card(
            PlayerId(0),
            Characteristics::basic_land("Forest"),
            Zone::Battlefield,
        );
        let cost = Cost {
            mana: None,
            components: vec![
                CostComponent::TapSelf,
                CostComponent::Sacrifice(SelectSpec {
                    zone: Zone::Battlefield,
                    filter: CardFilter::ItSelf,
                    chooser: PlayerRef::Controller,
                    min: ValueExpr::Fixed(1),
                    max: ValueExpr::Fixed(1),
                }),
            ],
        };

        // Payable while the source is on the battlefield.
        assert!(e.can_pay_cost(PlayerId(0), src, &cost));
        e.pay_cost(PlayerId(0), src, &cost);
        // The source is sacrificed: now in its owner's graveyard, off the battlefield.
        assert_eq!(e.state.object(src).zone, Zone::Graveyard, "source sacrificed to graveyard");
        assert!(e.state.player(PlayerId(0)).graveyard.contains(&src));
        assert!(!e.state.player(PlayerId(0)).battlefield.contains(&src));
        // No longer payable — there's nothing left to sacrifice.
        assert!(!e.can_pay_cost(PlayerId(0), src, &cost));
    }

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

        // The starting player (chosen by the opening die roll, CR 103.2–103.4) skips their
        // first draw (CR 103.8a).
        let starter = engine.state.active_player;
        engine.begin_turn();
        engine.run_step(Phase::Untap);
        engine.run_step(Phase::Upkeep);
        engine.run_step(Phase::Draw);
        assert_eq!(
            engine.state.player(starter).hand.len(),
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
        // Starting player is chosen by the opening die roll (CR 103.2); assert it alternates.
        let starter = engine.state.active_player;
        let other = engine
            .state
            .players
            .iter()
            .map(|p| p.id)
            .find(|&p| p != starter)
            .unwrap();
        assert_eq!(engine.state.turn_number, 1);
        engine.take_turn();
        assert_eq!(engine.state.active_player, other);
        assert_eq!(engine.state.turn_number, 2);
        engine.take_turn();
        assert_eq!(engine.state.active_player, starter);
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
            modes: Vec::new(),
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

    /// Says "yes" to every `Confirm` (and passes otherwise) — drives the pay-life branch of a
    /// shock land's ETB replacement.
    struct ConfirmYesAgent;
    impl Agent for ConfirmYesAgent {
        fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
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

    /// Worst case for repeatable-action loops (#55 root-cause hunt): at priority ALWAYS takes the
    /// first legal action and never voluntarily passes; aggressive in combat. A correct engine still
    /// terminates because resources deplete — if it spins, the loop-guard trips and names the loop.
    struct GreedyAgent;
    impl Agent for GreedyAgent {
        fn decide(&mut self, _view: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
            match req {
                DecisionRequest::Priority { actions, .. } => {
                    if actions.is_empty() {
                        DecisionResponse::Pass
                    } else {
                        DecisionResponse::Action(0)
                    }
                }
                DecisionRequest::ChooseTargets { slots, .. } => {
                    let pairs = slots
                        .iter()
                        .enumerate()
                        .filter(|(_, slot)| !slot.legal.is_empty())
                        .map(|(si, _)| (si as u32, 0u32))
                        .collect();
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
                DecisionRequest::ChooseModes { .. } => DecisionResponse::Indices(vec![0]),
                _ => DecisionResponse::Pass,
            }
        }
    }

    #[test]
    fn loop_guard_aborts_a_wedged_game_to_a_draw() {
        // #55 safety net: directly verify the loop-guard. Below the ceiling it is a no-op; at/above
        // it, it aborts the game to a DRAW (game_over, winner = None) so a wedged engine loop can
        // never hang — a single env.step() always returns.
        let state = crate::cards::two_player_demo_game(0);
        let mut e = pass_engine(state);
        assert!(!e.state.game_over);
        // One short of the ceiling → no-op, game still live.
        assert!(!e.loop_guard_tripped(PRIORITY_LOOP_LIMIT - 1, PRIORITY_LOOP_LIMIT, "test"));
        assert!(!e.state.game_over, "below the ceiling does nothing");
        // Reaching the ceiling → trips: returns true, game over as a draw.
        assert!(e.loop_guard_tripped(PRIORITY_LOOP_LIMIT, PRIORITY_LOOP_LIMIT, "test-loop"));
        assert!(e.state.game_over, "hitting the ceiling ends the game");
        assert_eq!(e.state.winner, None, "a loop-abort is a draw, not a win");
        // Idempotent: a second trip doesn't panic or change the (already drawn) result.
        assert!(e.loop_guard_tripped(PRIORITY_LOOP_LIMIT, PRIORITY_LOOP_LIMIT, "test-loop"));
        assert_eq!(e.state.winner, None);
    }

    #[test]
    fn greedy_demo_self_play_terminates() {
        // #55 guard: a DETERMINISTIC, never-voluntarily-pass policy is the worst case for
        // repeatable-action infinite loops (a `RandomAgent` masks them by passing at random). Every
        // game must still end — both with and without the Arena auto-pass profile, and well within
        // the engine loop-guard (no draw-by-loop-abort). Broader hunts found no trip: 12k random +
        // 1500 greedy (no auto-pass) and 4000 greedy/random (auto-pass on); this fast slice keeps the
        // property covered. If a future card adds a free repeatable action that wedges priority, this
        // is what catches it.
        for seed in 0..40u64 {
            for auto_pass in [false, true] {
                let state = crate::cards::two_player_demo_game(seed);
                let agents: Vec<Box<dyn Agent>> =
                    vec![Box::new(GreedyAgent), Box::new(GreedyAgent)];
                let mut engine = Engine::new(state, agents);
                engine.set_arena_auto_pass(auto_pass);
                engine.run_game();
                assert!(engine.state.game_over, "game must end (seed {seed}, auto_pass={auto_pass})");
            }
        }
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
    fn stop_policy_is_full_control_or_the_fixed_rule() {
        // The single stop knob (full control vs the fixed elision rule). Full control OFF (the
        // auto-pass profile on): stop ONLY when the seat has a meaningful (non-mana) action AND it's
        // a marked-stop step (empty stack) or an opponent's object is on the stack. Full control ON:
        // stop at every window. (The former separate SmartStops / resolve-own-stack knobs are gone.)
        let state = cards::build_game(1, &[&[], &[]]);
        let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
        e.set_arena_auto_pass(true); // full control OFF → the fixed rule
        e.state.active_player = PlayerId(0);
        let p0 = PlayerId(0);
        let set_phase = |e: &mut Engine, ph| e.state.phase = ph;

        // Marked stop (own main) WITH a meaningful action → stop; with nothing to do → auto-pass.
        set_phase(&mut e, Phase::PrecombatMain);
        assert!(!e.should_auto_pass(p0, true), "own MP1 with a play → stop");
        assert!(e.should_auto_pass(p0, false), "own MP1 with nothing to do → auto-pass");
        set_phase(&mut e, Phase::PostcombatMain);
        assert!(!e.should_auto_pass(p0, true), "own MP2 with a play → stop");

        // Non-stop steps auto-pass even WITH an action — the old "stop wherever you can act" is gone.
        set_phase(&mut e, Phase::Upkeep);
        assert!(e.should_auto_pass(p0, true), "upkeep is not a stop → auto-pass even with a play");
        set_phase(&mut e, Phase::CombatDamage);
        assert!(e.should_auto_pass(p0, true), "combat damage is not a stop → auto-pass");
        set_phase(&mut e, Phase::DeclareAttackers);
        assert!(e.should_auto_pass(p0, false), "declare-attackers is not a priority stop");

        // A manual stop makes an otherwise-elided step stop — when you have an action there.
        e.set_stop(p0, Phase::Upkeep, Some(true));
        set_phase(&mut e, Phase::Upkeep);
        assert!(!e.should_auto_pass(p0, true), "manual upkeep stop (with a play)");

        // Full control stops everywhere, action or not.
        e.set_arena_auto_pass(false); // full control ON for every seat
        set_phase(&mut e, Phase::End);
        assert!(!e.should_auto_pass(p0, false), "full control stops at the end step with nothing to do");
        set_phase(&mut e, Phase::Upkeep);
        assert!(!e.should_auto_pass(p0, false), "full control stops at every window");
    }

    #[test]
    fn fixed_rule_elides_minor_steps_but_stops_at_mains() {
        // End-to-end: with full control off, P0 (holding a land it can play in its main phases) is
        // prompted at its marked stops MP1/MP2 — a meaningful action is available there — but
        // auto-passed through every minor step (the land isn't playable at upkeep/draw/combat, so
        // there's no meaningful action and nothing to stop for).
        let prompted = Rc::new(RefCell::new(Vec::new()));
        let mut state = cards::build_game(1, &[&[], &[]]); // empty libraries
        let forest = state.card_db().get(crate::cards::grp::FOREST).unwrap().chars.clone();
        state.add_card(PlayerId(0), forest, Zone::Hand); // a main-phase play (the spy never plays it)
        let agents: Vec<Box<dyn Agent>> = vec![
            Box::new(PrioritySpy { prompted: Rc::clone(&prompted) }),
            Box::new(PassAgent),
        ];
        let mut e = Engine::new(state, agents);
        e.skip_opening_deal(); // no draw on turn 1 (P0 on the play); hand = the seeded Forest
        e.set_arena_auto_pass(true); // full control OFF → the fixed rule
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
            assert!(!seen.contains(&elided), "{elided:?} should be auto-passed (no play there)");
        }
        assert!(seen.contains(&Phase::PrecombatMain), "stop at own MP1 (a land to play)");
        assert!(seen.contains(&Phase::PostcombatMain), "stop at own MP2 (the land still in hand)");

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
        // PlayerView.stops echoes the seat's stop state (render-only): None under full control
        // (every window stops — nothing to elide), else `full_control` + the effective per-step stops.
        let state = cards::build_game(1, &[&[], &[]]);
        let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
        e.state.active_player = PlayerId(0);

        // Full control by default (headless) → nothing to elide → no stop state echoed.
        assert!(e.view_for_seat(PlayerId(0)).stops.is_none());

        e.set_arena_auto_pass(true); // full control OFF → the fixed elision rule is active
        e.set_stop(PlayerId(0), Phase::Upkeep, Some(true)); // manual extra stop
        let view = e.view_for_seat(PlayerId(0));
        let s = view.stops.expect("stops echoed when not under full control");
        assert!(!s.full_control, "the fixed elision profile is active");
        let stop_at = |ph: Phase| s.per_step.iter().find(|(p, _)| *p == ph).map(|(_, b)| *b);
        assert_eq!(stop_at(Phase::PrecombatMain), Some(true), "own MP1 default stop");
        assert_eq!(stop_at(Phase::PostcombatMain), Some(true), "own MP2 default stop");
        assert_eq!(stop_at(Phase::Upkeep), Some(true), "manual upkeep stop reflected");
        assert_eq!(stop_at(Phase::Draw), Some(false), "draw not a stop");
        // Untap/Cleanup grant no priority → not listed.
        assert!(stop_at(Phase::Untap).is_none());
    }

    #[test]
    fn your_own_object_on_the_stack_auto_passes() {
        // The fixed rule folds in the old resolve-own-stack behavior: while your own object is on
        // top of the stack you auto-pass (let it resolve, don't respond to yourself); an opponent is
        // still prompted to respond when they can act.
        use crate::stack::{StackObject, StackObjectKind};
        let state = cards::build_game(1, &[&[], &[]]);
        let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
        e.set_arena_auto_pass(true); // full control OFF → the fixed rule
        e.state.active_player = PlayerId(0);
        e.state.phase = Phase::PrecombatMain; // a marked stop — but your own object on top overrides
        let sid = e.state.mint_stack();
        e.state.stack.push(StackObject {
            id: sid,
            controller: PlayerId(0),
            source: None,
            kind: StackObjectKind::Ability { index: 0 },
            targets: vec![],
            x: None,
            modes: Vec::new(),
        });

        // P0's own object on top → auto-pass (even in MP1, even with an action available): a
        // non-empty stack with no OPPONENT object on top is never a stop.
        assert!(e.should_auto_pass(PlayerId(0), true), "auto-pass while your own object resolves");
        // The opponent is prompted to respond iff they can act.
        assert!(!e.should_auto_pass(PlayerId(1), true), "opponent prompted to respond (has play)");
        assert!(e.should_auto_pass(PlayerId(1), false), "opponent auto-passes with no response");
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
                    subtypes: vec![crate::subtypes::CreatureType::Bird.into()],
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
    fn cast_modal_chooses_mode_then_targets_only_that_mode() {
        // Bushwhack-shaped: a modal spell whose fight mode targets two creatures and whose other
        // mode has none. Casting it chooses the mode at 601.2b, then collects targets for ONLY
        // that mode at 601.2c (the new `Fight` arm in `collect_specs_into`), and resolution runs
        // only the chosen mode using the cast-locked modes/targets.
        use crate::basics::CardType;
        use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::{Effect, EffectTarget, Mode};
        use crate::state::Characteristics;
        use std::sync::Arc;

        let twotwo = |name: &str| Characteristics {
            name: name.into(),
            card_types: vec![CardType::Creature],
            power: Some(2),
            toughness: Some(2),
            ..Default::default()
        };
        let tspec = |f: CardFilter| TargetSpec { kind: TargetKind::Creature(f), min: 1, max: 1, distinct: true };
        let modal = Effect::Modal {
            modes: vec![
                Mode {
                    label: "Fight".into(),
                    effect: Effect::Fight {
                        a: EffectTarget::Target(tspec(CardFilter::ControlledBy(PlayerRef::Controller))),
                        b: EffectTarget::Target(tspec(CardFilter::Not(Box::new(
                            CardFilter::ControlledBy(PlayerRef::Controller),
                        )))),
                    },
                },
                Mode {
                    label: "Gain 3 life".into(),
                    effect: Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(3) },
                },
            ],
            min: 1,
            max: 1,
            allow_repeat: false,
        };
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Test Bushwhack".into(),
                card_types: vec![CardType::Sorcery],
                mana_cost: Some(cards::mana_cost(0, &[])),
                grp_id: 9300,
                ..Default::default()
            },
            abilities: vec![Ability::Spell { effect: modal }],
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let mine = state.add_card(PlayerId(0), twotwo("Mine"), Zone::Battlefield);
        let theirs = state.add_card(PlayerId(1), twotwo("Theirs"), Zone::Battlefield);
        let chars = state.card_db().get(9300).unwrap().chars.clone();
        let spell = state.add_card(PlayerId(0), chars, Zone::Hand);

        // Cast choosing mode 0 (Fight). ModeAgent answers ChooseModes; targets fall back to the
        // first legal candidate per slot (my creature vs their creature).
        let mut e = Engine::new(state, vec![Box::new(ModeAgent(0)), Box::new(PassAgent)]);
        e.cast_spell(PlayerId(0), spell, CastVariant::Normal);
        e.resolve_top();

        // The fight (and only the fight) ran: both 2/2s dealt 2 to each other.
        assert_eq!(e.state.object(mine).damage_marked, 2, "my creature took fight damage");
        assert_eq!(e.state.object(theirs).damage_marked, 2, "their creature took fight damage");
        assert_eq!(e.state.player(PlayerId(0)).life, 20, "the other mode (gain life) did NOT run");
    }

    #[test]
    fn modal_offers_only_legal_modes_no_empty_target_slot_cr_601_2c() {
        // #49 regression (found via Bushwhack in Selesnya self-play): a modal spell where mode 0 is a
        // fight (needs a creature you control AND one you don't) and mode 1 is untargeted. With NO
        // creatures in play, the fight mode is ILLEGAL (CR 700.2d), so the engine must offer ONLY the
        // untargeted mode and must NEVER emit a `ChooseTargets` carrying a required (min≥1) slot with
        // zero legal candidates (CR 601.2c). Before the fix the engine offered the fight mode anyway
        // and then asked for two creature targets that didn't exist.
        use crate::basics::CardType;
        use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::{Effect, EffectTarget, Mode};
        use crate::state::Characteristics;
        use std::sync::{Arc, Mutex};

        let tspec =
            |f: CardFilter| TargetSpec { kind: TargetKind::Creature(f), min: 1, max: 1, distinct: true };
        let modal = Effect::Modal {
            modes: vec![
                Mode {
                    label: "Fight".into(),
                    effect: Effect::Fight {
                        a: EffectTarget::Target(tspec(CardFilter::ControlledBy(PlayerRef::Controller))),
                        b: EffectTarget::Target(tspec(CardFilter::Not(Box::new(
                            CardFilter::ControlledBy(PlayerRef::Controller),
                        )))),
                    },
                },
                Mode {
                    label: "Gain 3 life".into(),
                    effect: Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(3) },
                },
            ],
            min: 1,
            max: 1,
            allow_repeat: false,
        };
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Test Bushwhack".into(),
                card_types: vec![CardType::Sorcery],
                mana_cost: Some(cards::mana_cost(0, &[])),
                grp_id: 9301,
                ..Default::default()
            },
            abilities: vec![Ability::Spell { effect: modal }],
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        // NO creatures anywhere — the fight mode has no legal targets for either slot.
        let chars = state.card_db().get(9301).unwrap().chars.clone();
        let spell = state.add_card(PlayerId(0), chars, Zone::Hand);

        // A recording agent: logs every request it's asked, answers ChooseModes by picking the first
        // offered (legal) option, and passes / min-selects otherwise.
        struct RecAgent(Arc<Mutex<Vec<DecisionRequest>>>);
        impl Agent for RecAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                self.0.lock().unwrap().push(req.clone());
                match req {
                    DecisionRequest::ChooseModes { .. } => DecisionResponse::Indices(vec![0]),
                    DecisionRequest::SelectCards { min, .. } => DecisionResponse::Indices((0..*min).collect()),
                    _ => DecisionResponse::Pass,
                }
            }
        }
        let log = Arc::new(Mutex::new(Vec::new()));
        let mut e = Engine::new(state, vec![Box::new(RecAgent(log.clone())), Box::new(PassAgent)]);

        // Unit-level: the fight mode is illegal (no creatures), the untargeted mode is legal, and the
        // spell is castable overall (via the legal mode) — CR 601.2c is satisfied without the fight.
        let effect = e.state.def_of(spell).unwrap().spell_effect().unwrap().clone();
        let modes = match &effect {
            Effect::Modal { modes, .. } => modes.clone(),
            _ => panic!("expected a modal effect"),
        };
        assert!(!e.mode_is_legal(&modes[0], PlayerId(0)), "fight mode illegal with no creatures");
        assert!(e.mode_is_legal(&modes[1], PlayerId(0)), "untargeted mode is always legal");
        assert!(e.spell_castable_targets(&effect, PlayerId(0)), "castable via the legal untargeted mode");

        e.cast_spell(PlayerId(0), spell, CastVariant::Normal);
        e.resolve_top();

        let reqs = log.lock().unwrap();
        // Exactly one ChooseModes was asked, offering ONLY the 1 legal mode (not both).
        let offered: Vec<usize> = reqs
            .iter()
            .filter_map(|r| match r {
                DecisionRequest::ChooseModes { modes, .. } => Some(modes.len()),
                _ => None,
            })
            .collect();
        assert_eq!(offered, vec![1], "one ChooseModes offering only the single legal mode");
        // No ChooseTargets with a required (min≥1) slot that has zero legal candidates was emitted.
        let leaked = reqs.iter().any(|r| {
            matches!(r, DecisionRequest::ChooseTargets { slots, .. }
                if slots.iter().any(|s| s.min >= 1 && s.legal.is_empty()))
        });
        assert!(!leaked, "no ChooseTargets with a required empty-legal slot (CR 601.2c)");
        // The legal mode (gain 3 life) is the one that resolved.
        assert_eq!(e.state.player(PlayerId(0)).life, 23, "the untargeted gain-life mode resolved");
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
                    CardFilter::Supertype(crate::subtypes::Supertype::Basic),
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
    fn earthbend_animates_a_land_and_adds_counters_c12() {
        use crate::basics::{CardType, Target};
        use crate::effects::ability::Keyword;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::value::ValueExpr;
        use crate::effects::{Effect, EffectTarget};
        // Earthbend 2 on a Forest you control: it becomes a 0/0 creature with haste that's still a
        // land, with two +1/+1 counters → a 2/2 land creature (which therefore survives the 0/0 SBA).
        let mut state = cards::build_game(1, &[&[], &[]]);
        let forest = put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield);
        let mut e = pass_engine(state);
        e.resolve_effect(
            &Effect::Earthbend { target: EffectTarget::ChosenIndex(0), n: ValueExpr::Fixed(2) },
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                source: Some(forest),
                chosen_targets: vec![Target::Object(forest)],
                ..Default::default()
            },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        let cc = e.state.computed(forest);
        assert!(cc.is_creature(), "the land became a creature");
        assert!(cc.card_types.contains(&CardType::Land), "and is still a land");
        assert!(cc.has_keyword(Keyword::Haste), "with haste");
        assert_eq!(cc.power, Some(2), "0/0 base + two +1/+1 counters");
        assert_eq!(cc.toughness, Some(2));
        assert_eq!(e.state.continuous_effects.len(), 1, "one floating animation effect registered");
        // It's a land creature: not marked to die by the 0/0 SBA, since it's a 2/2.
        assert!(
            !crate::sba::collect(&e.state).iter().any(|s| matches!(
                s,
                crate::sba::StateBasedAction::CreatureDies { creature, .. } if *creature == forest
            )),
            "a 2/2 land creature is not destroyed"
        );
    }

    #[test]
    fn earthbend_dies_trigger_returns_the_land_tapped_c12() {
        use crate::basics::{CardType, Target};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::value::ValueExpr;
        use crate::effects::{Effect, EffectTarget};
        // Earthbend 0 on a Forest → a 0/0 land creature, which the SBA kills (CR 704.5f). Its
        // delayed clause (CR 603.7) then returns it to the battlefield tapped — as a PLAIN land,
        // since the animation was pinned to the object that left (it must not follow the return).
        let mut state = cards::build_game(1, &[&[], &[]]);
        let forest = put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield);
        let mut e = pass_engine(state);
        e.resolve_effect(
            &Effect::Earthbend { target: EffectTarget::ChosenIndex(0), n: ValueExpr::Fixed(0) },
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                source: Some(forest),
                chosen_targets: vec![Target::Object(forest)],
                ..Default::default()
            },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        assert_eq!(e.state.delayed_triggers.len(), 1, "the return-tapped trigger is armed");
        assert!(e.state.computed(forest).is_creature(), "it's a 0/0 land creature");

        // The agenda kills the 0/0 (SBA), fires & consumes the delayed trigger, and stages it.
        e.run_agenda();
        assert!(e.state.delayed_triggers.is_empty(), "the dies trigger fired (consumed)");
        assert_eq!(e.state.object(forest).zone, Zone::Graveyard, "the 0/0 went to the graveyard");
        assert_eq!(e.state.stack.len(), 1, "the return-tapped delayed ability is on the stack");

        // Resolve the delayed ability → the land returns tapped.
        e.resolve_top();
        e.run_agenda();
        assert_eq!(e.state.object(forest).zone, Zone::Battlefield, "returned to the battlefield");
        assert!(e.state.object(forest).status.tapped, "and entered tapped");
        let cc = e.state.computed(forest);
        assert!(!cc.is_creature(), "it returns as a PLAIN land — the animation did not follow");
        assert!(cc.card_types.contains(&CardType::Land), "still a land");
        assert!(e.state.continuous_effects.is_empty(), "the floating animation effect was swept");
    }

    #[test]
    fn pump_doubles_power_until_end_of_turn_c15() {
        use crate::basics::Target;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::condition::Duration;
        use crate::effects::value::ValueExpr;
        use crate::effects::{Effect, EffectTarget};
        // "Double target creature's power until end of turn" (Mightform): a 2/2 → 4/2, then wears
        // off at cleanup. PumpPT{ power: PowerOfTarget(0), toughness: 0, UntilEndOfTurn } lowers to
        // a floating ModifyPT continuous effect snapshotting the target's power (CR 608.2h / 611).
        let mut state = cards::build_game(1, &[&[], &[]]);
        let bears = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        let mut e = pass_engine(state);
        assert_eq!(e.state.computed(bears).power, Some(2));
        e.resolve_effect(
            &Effect::PumpPT {
                what: EffectTarget::ChosenIndex(0),
                power: ValueExpr::PowerOfTarget(0),
                toughness: ValueExpr::Fixed(0),
                duration: Duration::UntilEndOfTurn,
            },
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(bears)],
                ..Default::default()
            },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        assert_eq!(e.state.computed(bears).power, Some(4), "power doubled (2 + its own snapshot 2)");
        assert_eq!(e.state.computed(bears).toughness, Some(2), "toughness unchanged");
        assert_eq!(e.state.continuous_effects.len(), 1, "one until-EOT pump is floating");

        // Cleanup (CR 514.2) ends "until end of turn" effects → back to 2/2.
        e.state.end_of_turn_continuous_cleanup();
        assert_eq!(e.state.computed(bears).power, Some(2), "the pump wore off at cleanup");
        assert!(e.state.continuous_effects.is_empty());
    }

    #[test]
    fn becomes_targeted_creature_spell_on_stack_half_surrak() {
        use crate::basics::Target;
        use crate::effects::ability::{Ability, EventPattern};
        use crate::effects::target::CardFilter;
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::Effect;
        use crate::stack::{StackObject, StackObjectKind};
        use std::sync::Arc;
        // Surrak's stack-half: "a creature SPELL you control becomes the target of a spell/ability an
        // opponent controls → draw." A creature spell on the stack, targeted by P1, fires P0's Surrak
        // — the becomes-targeted firing resolves a Target::Stack to the spell's card object (CR 603.2).
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Surrak (test)".into(),
                card_types: vec![CardType::Creature],
                power: Some(4),
                toughness: Some(3),
                grp_id: 9601,
                ..Default::default()
            },
            abilities: vec![Ability::Triggered {
                event: EventPattern::BecomesTargeted {
                    filter: CardFilter::All(vec![
                        CardFilter::HasCardType(CardType::Creature),
                        CardFilter::ControlledBy(PlayerRef::Controller),
                    ]),
                    by_opponent: true,
                },
                condition: None,
                intervening_if: false,
                effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
            }],
            text: String::new(),
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let surrak_chars = state.card_db().get(9601).unwrap().chars.clone();
        state.add_card(PlayerId(0), surrak_chars, Zone::Battlefield); // the watcher
        let bears_chars = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let spell_card = state.add_card(PlayerId(0), bears_chars, Zone::Stack); // a creature spell
        let lib_chars = state.card_db().get(grp::FOREST).unwrap().chars.clone();
        state.add_card(PlayerId(0), lib_chars, Zone::Library); // a card to draw
        let sid = state.mint_stack();
        state.stack.push(StackObject {
            id: sid,
            controller: PlayerId(0),
            source: Some(spell_card),
            kind: StackObjectKind::Spell(spell_card),
            targets: Vec::new(),
            x: None,
            modes: Vec::new(),
        });
        let mut e = pass_engine(state);

        // A Target::Stack resolves to the spell's card object for the becomes-targeted event.
        assert_eq!(
            e.targeted_object_ids(&[Target::Stack(sid)]),
            vec![spell_card],
            "the targeted creature spell resolves to its card object"
        );
        let before = e.state.player(PlayerId(0)).hand.len();
        // P1 targets the creature spell on the stack.
        e.fire_targeted(&[spell_card], PlayerId(1));
        e.run_agenda();
        e.resolve_top();
        assert_eq!(
            e.state.player(PlayerId(0)).hand.len(),
            before + 1,
            "Surrak draws when an opponent targets your creature spell on the stack"
        );
    }

    #[test]
    fn becomes_targeted_by_an_opponent_draws_c16() {
        use crate::effects::ability::{Ability, EventPattern};
        use crate::effects::target::CardFilter;
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::Effect;
        use std::sync::Arc;
        // Surrak-style C16: "Whenever a creature you control becomes the target of a spell or
        // ability an opponent controls, draw a card." Watcher P0; an opponent (P1) targeting P0's
        // creature draws, but P0 targeting its own creature does not (the by_opponent guard).
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Surrak (test)".into(),
                card_types: vec![CardType::Creature],
                power: Some(4),
                toughness: Some(3),
                grp_id: 9600,
                ..Default::default()
            },
            abilities: vec![Ability::Triggered {
                event: EventPattern::BecomesTargeted {
                    filter: CardFilter::All(vec![
                        CardFilter::HasCardType(CardType::Creature),
                        CardFilter::ControlledBy(PlayerRef::Controller),
                    ]),
                    by_opponent: true,
                },
                condition: None,
                intervening_if: false,
                effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
            }],
            text: String::new(),
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let surrak_chars = state.card_db().get(9600).unwrap().chars.clone();
        state.add_card(PlayerId(0), surrak_chars, Zone::Battlefield); // the watcher
        let bears_chars = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let bears = state.add_card(PlayerId(0), bears_chars, Zone::Battlefield);
        let lib_chars = state.card_db().get(grp::FOREST).unwrap().chars.clone();
        state.add_card(PlayerId(0), lib_chars, Zone::Library); // a card to draw

        let mut e = pass_engine(state);
        // P0 targeting its OWN creature: no trigger (the source isn't an opponent).
        e.broadcast(GameEvent::Targeted { object: bears, by: PlayerId(0) });
        assert!(e.state.pending_triggers.is_empty(), "targeting your own creature doesn't trigger");

        // An opponent (P1) targets P0's creature: triggers, and P0 draws.
        let before = e.state.player(PlayerId(0)).hand.len();
        e.broadcast(GameEvent::Targeted { object: bears, by: PlayerId(1) });
        assert_eq!(e.state.pending_triggers.len(), 1, "an opponent targeting your creature triggers");
        e.run_agenda(); // put the trigger on the stack
        e.resolve_top(); // resolve the draw
        assert_eq!(
            e.state.player(PlayerId(0)).hand.len(),
            before + 1,
            "you draw a card off the opponent targeting your creature"
        );
    }

    #[test]
    fn attack_triggers_fire_for_you_and_for_the_attacker() {
        use crate::effects::ability::{Ability, EventPattern};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::Effect;
        use std::sync::Arc;
        // Declaring an attack fires both "whenever you attack" (YouAttack, once for the attacking
        // player) and "whenever this creature attacks" (SelfAttacks, per attacker) triggers — the
        // attack-trigger wiring that was previously dead (SelfAttacks never fired). Unblocks Dyadrine.
        let draw1 = |grp: u32, name: &str, event: EventPattern| cards::CardDef {
            chars: Characteristics {
                name: name.into(),
                card_types: vec![CardType::Creature],
                power: Some(2),
                toughness: Some(2),
                grp_id: grp,
                ..Default::default()
            },
            abilities: vec![Ability::Triggered {
                event,
                condition: None,
                intervening_if: false,
                effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
            }],
            text: String::new(),
            ..Default::default()
        };
        let mut db = cards::starter_db();
        db.insert(draw1(9700, "Dyadrine (test)", EventPattern::YouAttack));
        db.insert(draw1(9701, "Raider (test)", EventPattern::SelfAttacks));
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let dyadrine = state.card_db().get(9700).unwrap().chars.clone();
        state.add_card(PlayerId(0), dyadrine, Zone::Battlefield); // the "you attack" watcher
        let raider_chars = state.card_db().get(9701).unwrap().chars.clone();
        let raider = state.add_card(PlayerId(0), raider_chars, Zone::Battlefield); // the attacker
        for _ in 0..2 {
            let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), f, Zone::Library);
        }
        let mut e = pass_engine(state);
        let before = e.state.player(PlayerId(0)).hand.len();
        // Simulate CR 508.1: `raider` is declared as an attacker by P0.
        e.broadcast(GameEvent::AttackersDeclared { attackers: vec![raider], by: PlayerId(0) });
        assert_eq!(e.state.pending_triggers.len(), 2, "you-attack + this-attacks both queued");
        e.run_agenda();
        e.resolve_top();
        e.resolve_top();
        assert_eq!(
            e.state.player(PlayerId(0)).hand.len(),
            before + 2,
            "drew off both the you-attack and the this-attacks trigger"
        );
    }

    #[test]
    fn search_candidates_are_revealed_to_the_searcher_43() {
        use crate::agent::{Agent, ObjView, SelectReason};
        use std::sync::{Arc, Mutex};
        // A SelectCards drawing candidates from the hidden library must surface those exact cards'
        // characteristics in the searcher's view (me.revealed_to_me) so the client can name/render
        // them (#43) — while the rest of the library stays masked.
        let mut state = cards::build_game(1, &[&[], &[]]);
        let forest_chars = state.card_db().get(grp::FOREST).unwrap().chars.clone();
        let mountain_chars = state.card_db().get(grp::MOUNTAIN).unwrap().chars.clone();
        let forest = state.add_card(PlayerId(0), forest_chars, Zone::Library); // a search candidate
        let hidden = state.add_card(PlayerId(0), mountain_chars, Zone::Library); // NOT in the request

        #[derive(Clone)]
        struct CaptureAgent(Arc<Mutex<Option<PlayerView>>>);
        impl Agent for CaptureAgent {
            fn decide(&mut self, view: &PlayerView, _req: &DecisionRequest) -> DecisionResponse {
                *self.0.lock().unwrap() = Some(view.clone());
                DecisionResponse::Indices(vec![0])
            }
        }
        let captured = Arc::new(Mutex::new(None));
        let agents: Vec<Box<dyn Agent>> =
            vec![Box::new(CaptureAgent(captured.clone())), Box::new(PassAgent)];
        let mut e = Engine::new(state, agents); // no start_game ⇒ library untouched
        let req = DecisionRequest::SelectCards {
            reason: SelectReason::Search,
            from: vec![forest],
            min: 1,
            max: 1,
            description: "search your library for a basic land".into(),
        };
        e.ask(PlayerId(0), &req);

        let view = captured.lock().unwrap().take().expect("the agent was asked");
        let named = |id: ObjId| {
            view.me.revealed_to_me.iter().find_map(|o| match o {
                ObjView::Visible { id: vid, chars, .. } if *vid == id => Some(chars.name.clone()),
                _ => None,
            })
        };
        assert_eq!(named(forest).as_deref(), Some("Forest"), "the candidate is revealed by name");
        assert!(named(hidden).is_none(), "the rest of the library stays masked");
    }

    #[test]
    fn warp_casts_cheap_then_exiles_at_end_step_c14() {
        use crate::basics::ManaCost;
        use crate::effects::ability::Ability;
        use std::collections::BTreeMap;
        use std::sync::Arc;
        // Warp (CR 702.x): a creature with warp {1} but a normal cost of {3}. With only 1 mana you
        // can't cast it normally, but you can warp it — it enters, then is exiled at the next end
        // step (pieces 1+2: alt-cost cast + the exile downside; recast-from-exile is a follow-up).
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Warp Creature (test)".into(),
                card_types: vec![CardType::Creature],
                mana_cost: Some(ManaCost { generic: 3, colored: BTreeMap::new(), x: 0 }),
                power: Some(2),
                toughness: Some(2),
                grp_id: 9950,
                ..Default::default()
            },
            abilities: vec![Ability::Warp {
                cost: ManaCost { generic: 1, colored: BTreeMap::new(), x: 0 },
            }],
            text: String::new(),
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let card = {
            let c = state.card_db().get(9950).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
        state.add_card(PlayerId(0), f, Zone::Battlefield); // a single mana
        let mut e = pass_engine(state);
        e.state.phase = Phase::PrecombatMain;

        // Only the warp cast is offered — 1 mana can't afford the normal {3}, but affords warp {1}.
        let actions = e.legal_priority_actions(PlayerId(0));
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, PlayableAction::Cast { variant: CastVariant::Warp, .. }))
                .count(),
            1,
            "warp cast is offered"
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, PlayableAction::Cast { variant: CastVariant::Normal, .. })),
            "the normal cast is not affordable"
        );

        // Cast it via warp; resolve → it enters and the exile-at-end-step trigger is armed.
        e.cast_spell(PlayerId(0), card, CastVariant::Warp);
        e.resolve_top();
        assert_eq!(e.state.object(card).zone, Zone::Battlefield, "the warp creature entered");
        assert_eq!(e.state.delayed_triggers.len(), 1, "exile-at-next-end-step is armed");

        // The end step begins → the delayed trigger fires and exiles it.
        e.broadcast(GameEvent::PhaseBegan { turn: 1, phase: Phase::End, active: PlayerId(0) });
        e.run_agenda();
        e.resolve_top();
        assert_eq!(e.state.object(card).zone, Zone::Exile, "exiled at the next end step");
        assert!(e.state.delayed_triggers.is_empty(), "the delayed trigger was consumed");
        assert!(e.state.object(card).castable_from_exile, "warp grants recast-from-exile");

        // Piece 3: a later turn — add untapped mana for the normal {3} and recast it from exile.
        e.state.phase = Phase::PrecombatMain;
        let forest_chars = e.state.card_db().get(grp::FOREST).unwrap().chars.clone();
        for _ in 0..3 {
            e.state.add_card(PlayerId(0), forest_chars.clone(), Zone::Battlefield);
        }
        let actions = e.legal_priority_actions(PlayerId(0));
        assert!(
            actions.iter().any(|a| matches!(
                a,
                PlayableAction::Cast { spell, variant: CastVariant::Normal } if *spell == card
            )),
            "the warp-exiled card is offered for recast from exile at its normal cost"
        );
        e.cast_spell(PlayerId(0), card, CastVariant::Normal);
        e.resolve_top();
        assert_eq!(e.state.object(card).zone, Zone::Battlefield, "recast from exile resolves");
        assert!(!e.state.player(PlayerId(0)).exile.contains(&card), "it left exile");
        assert!(e.state.delayed_triggers.is_empty(), "a normal recast does not re-arm a warp exile");
    }

    #[test]
    fn reflexive_when_you_do_targets_only_when_condition_met() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::CounterKind;
        use crate::effects::ability::{Ability, EventPattern};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::condition::Condition;
        use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::{Effect, EffectTarget};
        use std::sync::{Arc, Mutex};
        // Earthbender-style: "put a quest counter on this. When you do, if ≥4 quest counters, put a
        // +1/+1 on target creature you control." The reward is a reflexive trigger (CR 603.7c): its
        // target must be chosen ONLY at ≥4 (not on every sub-4 landfall), and the quest counter is
        // always added regardless of creatures — exactly the fidelity design flagged.
        let quest = CounterKind::Named("quest".into());
        let landfall = Effect::Sequence(vec![
            Effect::PutCounters {
                what: EffectTarget::SourceSelf,
                kind: quest.clone(),
                n: ValueExpr::Fixed(1),
            },
            Effect::Conditional {
                cond: Condition::ValueAtLeast(
                    ValueExpr::CountersOnSelf(quest.clone()),
                    ValueExpr::Fixed(4),
                ),
                then: Box::new(Effect::PutCounters {
                    what: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::Fixed(1),
                }),
                otherwise: None,
            },
        ]);

        // Records ChooseTargets prompts; picks the first legal target.
        #[derive(Clone)]
        struct TgtSpy(Arc<Mutex<u32>>);
        impl Agent for TgtSpy {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::ChooseTargets { slots, .. } => {
                        *self.0.lock().unwrap() += 1;
                        DecisionResponse::Pairs(
                            slots
                                .iter()
                                .enumerate()
                                .filter(|(_, s)| s.min > 0)
                                .map(|(i, _)| (i as u32, 0))
                                .collect(),
                        )
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        // `start_quest` = the counters before the landfall (+1 makes it `start_quest+1`).
        let run = |start_quest: u32| -> (i32, u32) {
            let mut db = cards::starter_db();
            db.insert(cards::CardDef {
                chars: Characteristics {
                    name: "Ascension (test)".into(),
                    card_types: vec![CardType::Enchantment],
                    grp_id: 9990,
                    ..Default::default()
                },
                abilities: vec![Ability::Triggered {
                    event: EventPattern::SelfEnters,
                    condition: None,
                    intervening_if: false,
                    effect: landfall.clone(),
                }],
                text: String::new(),
                ..Default::default()
            });
            let mut state = GameState::new(2, 1);
            state.set_card_db(Arc::new(db));
            let asc = {
                let c = state.card_db().get(9990).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            let creature = {
                let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            state.objects.get_mut(&asc).unwrap().counters.counts.insert(quest.clone(), start_quest);
            let prompts = Arc::new(Mutex::new(0u32));
            let agents: Vec<Box<dyn Agent>> =
                vec![Box::new(TgtSpy(prompts.clone())), Box::new(PassAgent)];
            let mut e = Engine::new(state, agents);
            e.resolve_effect(
                &landfall,
                &ResolutionCtx {
                    controller: Some(PlayerId(0)),
                    source: Some(asc),
                    ability_index: Some(0),
                    ..Default::default()
                },
                WbReason::Resolve(crate::ids::StackId(0)),
            );
            e.run_agenda(); // reflexive onto the stack (choosing its target only if ≥4)
            e.resolve_top(); // resolve the reflexive reward
            e.run_agenda();
            let power = e.state.computed(creature).power.unwrap_or(0);
            let n = *prompts.lock().unwrap();
            (power, n)
        };

        // Starting at 3 → +1 = 4 ≥ 4 → reward fires: the creature is buffed (2/2 → 3/3), one prompt.
        let (power_at4, prompts_at4) = run(3);
        assert_eq!(power_at4, 3, "≥4 quest counters → +1/+1 on the target creature");
        assert_eq!(prompts_at4, 1, "the reward target was chosen (one prompt)");

        // Starting at 1 → +1 = 2 < 4 → no reward AND no target prompt (the key fidelity fix).
        let (power_at2, prompts_at2) = run(1);
        assert_eq!(power_at2, 2, "< 4 → no +1/+1");
        assert_eq!(prompts_at2, 0, "and crucially NO target prompt on a sub-4 landfall");
    }

    #[test]
    fn extra_land_plays_and_play_lands_from_graveyard_c18() {
        use crate::effects::ability::{Ability, StaticContribution};
        use crate::effects::condition::Duration;
        use crate::effects::target::{CardFilter, SelectSpec};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use std::sync::Arc;
        // Icetill Explorer: "you may play an additional land each turn" + "play lands from your
        // graveyard" — two player-level static permissions (C18) read by the land-play legality.
        let perm = |c: StaticContribution| Ability::Static {
            contribution: c,
            affects: SelectSpec {
                zone: Zone::Battlefield,
                filter: CardFilter::ControlledBy(PlayerRef::Controller),
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(0),
                max: ValueExpr::Fixed(0),
            },
            duration: Duration::WhileSourcePresent,
        };
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Icetill (test)".into(),
                card_types: vec![CardType::Creature],
                power: Some(2),
                toughness: Some(4),
                grp_id: 9955,
                ..Default::default()
            },
            abilities: vec![
                perm(StaticContribution::ExtraLandPlays(1)),
                perm(StaticContribution::PlayLandsFrom(Zone::Graveyard)),
            ],
            text: String::new(),
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        {
            let c = state.card_db().get(9955).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        for zone in [Zone::Hand, Zone::Graveyard] {
            let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, zone);
        }
        let mut e = pass_engine(state);
        e.state.phase = Phase::PrecombatMain;

        let land_plays = |e: &Engine| {
            e.legal_priority_actions(PlayerId(0))
                .iter()
                .filter(|a| matches!(a, PlayableAction::PlayLand { .. }))
                .count()
        };
        // Both the hand land AND the graveyard land are playable (play-from-graveyard permission).
        assert_eq!(land_plays(&e), 2, "hand + graveyard lands are playable");
        // After one land this turn, still playable — the extra-land permission allows a 2nd.
        e.state.player_mut(PlayerId(0)).lands_played_this_turn = 1;
        assert_eq!(land_plays(&e), 2, "a second land drop is still allowed");
        // After two, the (1 base + 1 extra) limit is reached.
        e.state.player_mut(PlayerId(0)).lands_played_this_turn = 2;
        assert_eq!(land_plays(&e), 0, "the land-play limit is reached");
    }

    #[test]
    fn dyadrine_optional_foreach_removes_counters_draws_and_makes_token() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::CounterKind;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::target::{CardFilter, SelectSpec, TokenSpec};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::{Effect, EffectTarget};
        use std::sync::Arc;
        // Dyadrine c3: "you may remove a +1/+1 counter from each of two creatures you control. If you
        // do, draw a card + make a 2/2 Robot." Optional (you may) + ForEach over a Select (2 distinct
        // creatures you control with a counter — a resolution-time choice, not targeting) + reward.
        let pp = CounterKind::PlusOnePlusOne;
        let effect = Effect::Optional {
            prompt: "remove?".into(),
            body: Box::new(Effect::Sequence(vec![
                Effect::ForEach {
                    selector: SelectSpec {
                        zone: Zone::Battlefield,
                        filter: CardFilter::All(vec![
                            CardFilter::HasCardType(CardType::Creature),
                            CardFilter::HasCounter(pp.clone()),
                        ]),
                        chooser: PlayerRef::Controller,
                        min: ValueExpr::Fixed(2),
                        max: ValueExpr::Fixed(2),
                    },
                    body: Box::new(Effect::PutCounters {
                        what: EffectTarget::Each,
                        kind: pp.clone(),
                        n: ValueExpr::Fixed(-1),
                    }),
                },
                Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
                Effect::CreateToken {
                    spec: TokenSpec {
                        name: "Robot".into(),
                        card_types: vec![CardType::Artifact, CardType::Creature],
                        subtypes: vec![],
                        colors: vec![],
                        power: 2,
                        toughness: 2,
                        keywords: vec![],
                        counters: vec![],
                    },
                    count: ValueExpr::Fixed(1),
                    controller: PlayerRef::Controller,
                },
            ])),
        };

        #[derive(Clone)]
        struct YesAgent;
        impl Agent for YesAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::Confirm { .. } => DecisionResponse::Bool(true),
                    DecisionRequest::SelectCards { min, .. } => {
                        DecisionResponse::Indices((0..*min).collect())
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        let mut state = cards::build_game(1, &[&[], &[]]);
        let c1 = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let c2 = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        state.objects.get_mut(&c1).unwrap().counters.counts.insert(pp.clone(), 1);
        state.objects.get_mut(&c2).unwrap().counters.counts.insert(pp.clone(), 1);
        let lib = {
            let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), f, Zone::Library)
        };
        let before = state.objects.len();
        let mut e = Engine::new(state, vec![Box::new(YesAgent), Box::new(YesAgent)]);

        e.resolve_effect(
            &effect,
            &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
            WbReason::Resolve(crate::ids::StackId(0)),
        );

        assert_eq!(e.state.object(c1).counters.get(&pp), 0, "c1 lost its +1/+1 counter");
        assert_eq!(e.state.object(c2).counters.get(&pp), 0, "c2 lost its +1/+1 counter");
        assert!(e.state.player(PlayerId(0)).hand.contains(&lib), "and you drew a card");
        assert_eq!(e.state.objects.len(), before + 1, "and made a Robot token");
    }

    #[test]
    fn crew_taps_creatures_and_animates_the_vehicle() {
        use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
        use crate::effects::condition::Duration;
        use crate::effects::{Effect, EffectTarget};
        use crate::subtypes::ArtifactType;
        use std::sync::Arc;
        // Crew 4 (CR 702.122): tap creatures with total power ≥4 → the Vehicle becomes an artifact
        // creature until end of turn (it already has P/T + is an artifact, so just AddType(Creature)).
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Wagon (test)".into(),
                card_types: vec![CardType::Artifact],
                subtypes: vec![ArtifactType::Vehicle.into()],
                power: Some(4),
                toughness: Some(4),
                grp_id: 9960,
                ..Default::default()
            },
            abilities: vec![Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::Crew(4)] },
                effect: Effect::BecomeCreature {
                    what: EffectTarget::SourceSelf,
                    duration: Duration::UntilEndOfTurn,
                },
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            }],
            text: String::new(),
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let wagon = {
            let c = state.card_db().get(9960).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let c1 = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        let c2 = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield); // 2/2
        let mut e = pass_engine(state);

        assert!(!e.state.computed(wagon).is_creature(), "an uncrewed Vehicle is not a creature");
        // Crew 4 is payable (two 2/2s total power 4) — activate it.
        assert!(e.can_pay_cost(
            PlayerId(0),
            wagon,
            &Cost { mana: None, components: vec![CostComponent::Crew(4)] }
        ));
        e.activate_ability(PlayerId(0), wagon, crate::agent::AbilityRef(0));
        e.resolve_top();
        assert!(e.state.computed(wagon).is_creature(), "crewed → an artifact creature");
        assert!(
            e.state.object(c1).status.tapped && e.state.object(c2).status.tapped,
            "the crewing creatures are tapped"
        );

        e.state.end_of_turn_continuous_cleanup();
        assert!(!e.state.computed(wagon).is_creature(), "no longer a creature after end of turn");
    }

    #[test]
    fn fabled_passage_untaps_the_fetched_land_at_four_lands() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{CardType, ZoneDest, ZonePos};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::condition::Condition;
        use crate::effects::target::CardFilter;
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::{Effect, EffectTarget};
        use crate::subtypes::Supertype;
        // Fabled Passage's tail: "Search for a basic land, put it onto the battlefield tapped, then
        // if you control 4+ lands, untap that land." `EffectTarget::Searched(0)` references the
        // just-fetched permanent; the untap is gated by an inline `Conditional` (CR — count after).
        let effect = Effect::Sequence(vec![
            Effect::Search {
                who: PlayerRef::Controller,
                zone: Zone::Library,
                filter: CardFilter::All(vec![
                    CardFilter::HasCardType(CardType::Land),
                    CardFilter::Supertype(Supertype::Basic),
                ]),
                min: 0,
                max: 1,
                to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
                tapped: true,
            },
            Effect::Conditional {
                cond: Condition::CountAtLeast {
                    zone: Zone::Battlefield,
                    filter: CardFilter::HasCardType(CardType::Land),
                    controller: Some(PlayerRef::Controller),
                    n: ValueExpr::Fixed(4),
                },
                then: Box::new(Effect::Tap { what: EffectTarget::Searched(0), tap: false }),
                otherwise: None,
            },
        ]);

        struct FetchAgent;
        impl Agent for FetchAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { from, .. } => {
                        DecisionResponse::Indices(if from.is_empty() { vec![] } else { vec![0] })
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        // Returns whether the fetched land is tapped (None if it wasn't fetched).
        let run = |pre_lands: usize| -> Option<bool> {
            let mut state = cards::build_game(1, &[&[], &[]]);
            for _ in 0..pre_lands {
                let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
                state.add_card(PlayerId(0), f, Zone::Battlefield);
            }
            let lib_forest = {
                let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
                state.add_card(PlayerId(0), f, Zone::Library)
            };
            let mut e = Engine::new(state, vec![Box::new(FetchAgent), Box::new(PassAgent)]);
            e.resolve_effect(
                &effect,
                &ResolutionCtx { controller: Some(PlayerId(0)), ..Default::default() },
                WbReason::Resolve(crate::ids::StackId(0)),
            );
            let o = e.state.object(lib_forest);
            (o.zone == Zone::Battlefield).then_some(o.status.tapped)
        };

        // 3 pre-lands + the fetched = 4 ≥ 4 → the fetched land is untapped.
        assert_eq!(run(3), Some(false), "≥4 lands → untap the fetched land");
        // 2 pre-lands + the fetched = 3 < 4 → it stays tapped.
        assert_eq!(run(2), Some(true), "< 4 lands → the fetched land stays tapped");
    }

    #[test]
    fn fabled_passage_untaps_via_full_activation() {
        // #58: the direct-resolve test above passes, but the user saw the fetched land stay tapped in
        // a REAL game. Drive the WHOLE path — activate the `{T}, Sacrifice this:` ability (pays the
        // cost incl. the sacrifice, which removes Fabled Passage from the battlefield), then resolve.
        use crate::agent::{AbilityRef, Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::cards::eld::fabled_passage::FABLED_PASSAGE;
        use std::sync::Arc;

        struct FetchAgent;
        impl Agent for FetchAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { from, .. } => {
                        DecisionResponse::Indices(if from.is_empty() { vec![] } else { vec![0] })
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        // `other_lands` = OTHER lands on the battlefield (besides Fabled Passage, which is sacrificed).
        let run = |other_lands: usize| -> Option<bool> {
            let mut state = GameState::new(2, 1);
            state.set_card_db(Arc::new(cards::starter_db())); // includes Fabled Passage (eld::register)
            let fabled = {
                let c = state.card_db().get(FABLED_PASSAGE).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            for _ in 0..other_lands {
                let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
                state.add_card(PlayerId(0), f, Zone::Battlefield);
            }
            let lib = {
                let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
                state.add_card(PlayerId(0), f, Zone::Library)
            };
            let mut e = Engine::new(state, vec![Box::new(FetchAgent), Box::new(PassAgent)]);
            e.activate_ability(PlayerId(0), fabled, AbilityRef(0));
            e.resolve_top();
            let o = e.state.object(lib);
            (o.zone == Zone::Battlefield).then_some(o.status.tapped)
        };

        // Fabled Passage is sacrificed (cost), so 3 OTHER lands + the fetched = 4 ≥ 4 → untap.
        assert_eq!(run(3), Some(false), "3 other lands + fetched = 4 → untap the fetched land");
        // 2 other lands + the fetched = 3 < 4 → stays tapped.
        assert_eq!(run(2), Some(true), "2 other lands + fetched = 3 → stays tapped");
    }

    #[test]
    fn erode_destroys_target_before_its_controller_fetches_a_land_61() {
        // #61: Erode = Sequence[Destroy target creature, its controller fetches a basic]. The Destroy
        // is a DEFERRED whiteboard action; the fetch (Search) is IMPERATIVE. Without ordering across
        // that boundary, the fetched land would enter while the doomed creature is still on the
        // battlefield, wrongly firing its landfall. The fix flushes the Destroy before the imperative
        // fetch, so a landfall creature is gone before its controller's land enters → no trigger.
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{CardType, CounterKind, Target, ZoneDest, ZonePos};
        use crate::cards::helpers::land_you_control;
        use crate::effects::ability::{Ability, EventPattern};
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::{Effect, EffectTarget};
        use crate::state::Characteristics;
        use crate::subtypes::Supertype;
        use std::sync::Arc;

        let erode = Effect::Sequence(vec![
            Effect::Destroy {
                what: EffectTarget::Target(TargetSpec {
                    kind: TargetKind::Permanent(CardFilter::HasCardType(CardType::Creature)),
                    min: 1,
                    max: 1,
                    distinct: true,
                }),
            },
            Effect::Search {
                who: PlayerRef::ControllerOfTarget(0),
                zone: Zone::Library,
                filter: CardFilter::All(vec![
                    CardFilter::HasCardType(CardType::Land),
                    CardFilter::Supertype(Supertype::Basic),
                ]),
                min: 0,
                max: 1,
                to: ZoneDest { zone: Zone::Battlefield, pos: ZonePos::Any },
                tapped: true,
            },
        ]);

        struct FetchAgent;
        impl Agent for FetchAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::SelectCards { from, .. } => {
                        DecisionResponse::Indices(if from.is_empty() { vec![] } else { vec![0] })
                    }
                    _ => DecisionResponse::Pass,
                }
            }
        }

        // Returns (creature destroyed, land fetched, # triggers queued by the resolution).
        let run = |landfall: bool| -> (bool, bool, usize) {
            let mut db = cards::starter_db();
            let abilities = if landfall {
                vec![Ability::Triggered {
                    event: EventPattern::PermanentEnters(land_you_control()),
                    condition: None,
                    intervening_if: false,
                    effect: Effect::PutCounters {
                        what: EffectTarget::SourceSelf,
                        kind: CounterKind::PlusOnePlusOne,
                        n: ValueExpr::Fixed(1),
                    },
                }]
            } else {
                Vec::new()
            };
            db.insert(cards::CardDef {
                chars: Characteristics {
                    name: "Erode Target".into(),
                    card_types: vec![CardType::Creature],
                    power: Some(2),
                    toughness: Some(2),
                    grp_id: 9980,
                    ..Default::default()
                },
                abilities,
                text: String::new(),
                ..Default::default()
            });
            let mut state = GameState::new(2, 1);
            state.set_card_db(Arc::new(db));
            let creature = {
                let c = state.card_db().get(9980).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            let lib = {
                let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
                state.add_card(PlayerId(0), f, Zone::Library)
            };
            let mut e = Engine::new(state, vec![Box::new(FetchAgent), Box::new(PassAgent)]);
            e.resolve_effect(
                &erode,
                &ResolutionCtx {
                    controller: Some(PlayerId(0)),
                    chosen_targets: vec![Target::Object(creature)],
                    target_controllers: vec![Some(PlayerId(0))],
                    ..Default::default()
                },
                WbReason::Resolve(crate::ids::StackId(0)),
            );
            (
                e.state.object(creature).zone == Zone::Graveyard,
                e.state.object(lib).zone == Zone::Battlefield,
                e.state.pending_triggers.len(),
            )
        };

        // The doomed landfall creature is destroyed BEFORE the fetched land enters → its landfall
        // does NOT fire (0 triggers queued), yet the land is still fetched.
        assert_eq!(run(true), (true, true, 0), "landfall creature gone before fetch → no landfall");
        // A vanilla creature: destroyed + land fetched, no triggers (sanity).
        assert_eq!(run(false), (true, true, 0), "vanilla creature: destroyed, land fetched");
    }

    #[test]
    fn ba_sing_se_taps_itself_for_tap_plus_other_lands_for_mana_57() {
        // #57: a "{2}{G}, {T}: ..." land — its `{T}` taps itself, so its own `{T}: Add {G}` mana
        // ability can't ALSO pay a `{G}` of the `{2}{G}`. With the pool rework, paying the non-mana
        // `{T}` first excludes the source from the mana set → the `{2}{G}` comes from 3 OTHER lands.
        use crate::agent::AbilityRef;
        use crate::basics::{CardType, Color};
        use crate::effects::ability::{Ability, Cost, CostComponent, Timing};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::Effect;
        use crate::state::Characteristics;
        use std::sync::Arc;

        let build = |forests: usize| -> (Engine, ObjId, Vec<ObjId>) {
            let mut db = cards::starter_db();
            db.insert(cards::CardDef {
                chars: Characteristics {
                    name: "Ba Sing Se (test)".into(),
                    card_types: vec![CardType::Land],
                    grp_id: 9990,
                    ..Default::default()
                },
                abilities: vec![
                    cards::mana_ability(Color::Green), // [0] {T}: Add {G} — makes it a mana source
                    Ability::Activated {
                        // [1] {2}{G}, {T}: gain 1 life (stand-in for Earthbend 2)
                        cost: Cost {
                            mana: Some(cards::mana_cost(2, &[(Color::Green, 1)])),
                            components: vec![CostComponent::TapSelf],
                        },
                        effect: Effect::GainLife {
                            who: PlayerRef::Controller,
                            amount: ValueExpr::Fixed(1),
                        },
                        timing: Timing::Instant,
                        restriction: None,
                        is_mana: false,
                    },
                ],
                text: String::new(),
                ..Default::default()
            });
            let mut state = GameState::new(2, 1);
            state.set_card_db(Arc::new(db));
            let bss = {
                let c = state.card_db().get(9990).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            };
            let forest_ids: Vec<ObjId> = (0..forests)
                .map(|_| {
                    let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
                    state.add_card(PlayerId(0), f, Zone::Battlefield)
                })
                .collect();
            let e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
            (e, bss, forest_ids)
        };
        let cost_of = |e: &Engine, bss: ObjId| match &e.state.def_of(bss).unwrap().abilities[1] {
            Ability::Activated { cost, .. } => cost.clone(),
            _ => unreachable!(),
        };

        // 3 OTHER lands → the `{2}{G}` is affordable; activating taps Ba Sing Se (for `{T}`) plus
        // exactly the 3 Forests (for `{2}{G}`), never Ba Sing Se for both.
        let (mut e3, bss3, forests3) = build(3);
        assert!(e3.can_pay_cost(PlayerId(0), bss3, &cost_of(&e3, bss3)), "3 other lands afford it");
        e3.activate_ability(PlayerId(0), bss3, AbilityRef(1));
        e3.resolve_top();
        assert!(e3.state.object(bss3).status.tapped, "Ba Sing Se tapped for its own {{T}}");
        assert_eq!(
            forests3.iter().filter(|&&id| e3.state.object(id).status.tapped).count(),
            3,
            "the {{2}}{{G}} came from all 3 OTHER lands (not Ba Sing Se double-tapped)"
        );

        // Only 2 OTHER lands → NOT affordable: Ba Sing Se can't pay both its `{T}` and a `{G}`.
        let (e2, bss2, _) = build(2);
        assert!(
            !e2.can_pay_cost(PlayerId(0), bss2, &cost_of(&e2, bss2)),
            "2 other lands can't pay {{2}}{{G}} once Ba Sing Se is committed to {{T}}"
        );
    }

    #[test]
    fn audit_harness_declare_attackers_explicit_and_resolve_to_stable() {
        // Validates the #60 attack-trigger harness primitives: `declare_attackers_explicit` (declare a
        // specific attacker, bypassing the agent prompt) fires `AttackersDeclared`, and
        // `resolve_to_stable` drains the resulting trigger (queue via run_agenda → resolve_top). A
        // "whenever you attack, gain 2 life" creature → attacking gains the life.
        use crate::basics::CardType;
        use crate::effects::ability::{Ability, EventPattern};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::effects::Effect;
        use crate::state::Characteristics;
        use std::sync::Arc;

        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Attack Pinger".into(),
                card_types: vec![CardType::Creature],
                power: Some(2),
                toughness: Some(2),
                grp_id: 9985,
                ..Default::default()
            },
            abilities: vec![Ability::Triggered {
                event: EventPattern::YouAttack,
                condition: None,
                intervening_if: false,
                effect: Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(2) },
            }],
            text: String::new(),
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let atkr = {
            let c = state.card_db().get(9985).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        state.objects.get_mut(&atkr).unwrap().summoning_sick = false;
        state.active_player = PlayerId(0);
        let mut e = Engine::new(state, vec![Box::new(PassAgent), Box::new(PassAgent)]);
        let life_before = e.state.player(PlayerId(0)).life;

        e.declare_attackers_explicit(&[atkr]); // fires AttackersDeclared → queues the YouAttack trigger
        e.resolve_to_stable(); // run_agenda stacks it, resolve_top resolves it

        assert_eq!(
            e.state.player(PlayerId(0)).life,
            life_before + 2,
            "the 'whenever you attack' trigger resolved and gained 2 life"
        );
        assert!(e.state.object(atkr).status.tapped, "the attacker tapped (no vigilance)");
    }

    #[test]
    fn conditional_gates_grant_keyword_until_eot() {
        use crate::basics::{CounterKind, Target};
        use crate::effects::ability::Keyword;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::condition::{Condition, Duration};
        use crate::effects::value::ValueExpr;
        use crate::effects::{Effect, EffectTarget};
        // "If [source] has ≥4 quest counters, target creature gains trample until end of turn."
        // Effect::Conditional (source-aware intervening-if) gates Effect::GrantKeyword — the two
        // reusable caps behind Earthbender's reward. Wears off at cleanup (CR 514.2).
        let mut state = cards::build_game(1, &[&[], &[]]);
        let src = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let creature = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let mut e = pass_engine(state);
        let quest = CounterKind::Named("quest".into());

        let effect = Effect::Conditional {
            cond: Condition::ValueAtLeast(
                ValueExpr::CountersOnSelf(quest.clone()),
                ValueExpr::Fixed(4),
            ),
            then: Box::new(Effect::GrantKeyword {
                what: EffectTarget::ChosenIndex(0),
                keyword: Keyword::Trample,
                duration: Duration::UntilEndOfTurn,
            }),
            otherwise: None,
        };
        let run = |e: &mut Engine| {
            e.resolve_effect(
                &effect,
                &ResolutionCtx {
                    controller: Some(PlayerId(0)),
                    source: Some(src),
                    chosen_targets: vec![Target::Object(creature)],
                    ..Default::default()
                },
                WbReason::Resolve(crate::ids::StackId(0)),
            );
        };

        // 3 quest counters → condition false → no grant.
        e.state.objects.get_mut(&src).unwrap().counters.counts.insert(quest.clone(), 3);
        run(&mut e);
        assert!(!e.state.computed(creature).has_keyword(Keyword::Trample), "3 < 4 → no trample");

        // 4 quest counters → condition true → trample granted until end of turn.
        e.state.objects.get_mut(&src).unwrap().counters.counts.insert(quest.clone(), 4);
        run(&mut e);
        assert!(e.state.computed(creature).has_keyword(Keyword::Trample), "≥4 → trample granted");

        e.state.end_of_turn_continuous_cleanup();
        assert!(
            !e.state.computed(creature).has_keyword(Keyword::Trample),
            "the granted trample wears off at end of turn"
        );
    }

    #[test]
    fn exile_target_card_from_a_graveyard_c17() {
        use crate::basics::Target;
        use crate::effects::action::{ResolutionCtx, WbReason};
        use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
        use crate::effects::{Effect, EffectTarget};
        // Keen-Eyed Curator's "{1}: Exile target card from a graveyard." A card in a graveyard is a
        // legal CardInZone{Graveyard} candidate, and Effect::Exile moves it to its owner's exile.
        let mut state = cards::build_game(1, &[&[], &[]]);
        let gy_chars = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        let card = state.add_card(PlayerId(1), gy_chars, Zone::Graveyard);
        let mut e = pass_engine(state);

        let spec = TargetSpec {
            kind: TargetKind::CardInZone { zone: Zone::Graveyard, filter: CardFilter::Any },
            min: 1,
            max: 1,
            distinct: true,
        };
        assert_eq!(
            e.target_candidates(&spec, PlayerId(0)),
            vec![Target::Object(card)],
            "a graveyard card is a legal 'target card from a graveyard'"
        );

        e.resolve_effect(
            &Effect::Exile { what: EffectTarget::ChosenIndex(0) },
            &ResolutionCtx {
                controller: Some(PlayerId(0)),
                chosen_targets: vec![Target::Object(card)],
                ..Default::default()
            },
            WbReason::Resolve(crate::ids::StackId(0)),
        );
        assert_eq!(e.state.object(card).zone, Zone::Exile, "the card was exiled");
        assert!(e.state.player(PlayerId(1)).exile.contains(&card), "into its owner's exile");
        assert!(!e.state.player(PlayerId(1)).graveyard.contains(&card), "out of the graveyard");
    }

    #[test]
    fn conditional_static_buffs_on_four_exiled_card_types_c17() {
        use crate::effects::ability::{Ability, Keyword, StaticContribution};
        use crate::effects::condition::{Condition, Duration};
        use crate::effects::target::{CardFilter, SelectSpec};
        use crate::effects::value::{PlayerRef, ValueExpr};
        use std::sync::Arc;
        // Keen-Eyed Curator's static: "+4/+4 and has trample as long as there are four or more card
        // types among cards exiled with this creature." Two ConditionalStatic on ItSelf, gated on
        // `ValueAtLeast(DistinctCardTypesAmongExiledWith, 4)`, evaluated relative to the source.
        let cond = Condition::ValueAtLeast(
            ValueExpr::DistinctCardTypesAmongExiledWith,
            ValueExpr::Fixed(4),
        );
        let itself = || SelectSpec {
            zone: Zone::Battlefield,
            filter: CardFilter::ItSelf,
            chooser: PlayerRef::Controller,
            min: ValueExpr::Fixed(0),
            max: ValueExpr::Fixed(0),
        };
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Keen-Eyed (test)".into(),
                card_types: vec![CardType::Creature],
                power: Some(3),
                toughness: Some(3),
                grp_id: 9900,
                ..Default::default()
            },
            abilities: vec![
                Ability::ConditionalStatic {
                    contribution: StaticContribution::ModifyPT { power: 4, toughness: 4 },
                    affects: itself(),
                    duration: Duration::WhileSourcePresent,
                    condition: cond.clone(),
                },
                Ability::ConditionalStatic {
                    contribution: StaticContribution::GrantKeyword(Keyword::Trample),
                    affects: itself(),
                    duration: Duration::WhileSourcePresent,
                    condition: cond.clone(),
                },
            ],
            text: String::new(),
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let keen = {
            let c = state.card_db().get(9900).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield)
        };
        let mut exile_typed = |state: &mut GameState, t: CardType| {
            let chars = Characteristics { card_types: vec![t], ..Default::default() };
            let id = state.add_card(PlayerId(0), chars, Zone::Exile);
            state.objects.get_mut(&id).unwrap().exiled_with = Some(keen);
            state.mark_chars_dirty();
        };

        assert_eq!(state.computed(keen).power, Some(3), "no exiled cards → plain 3/3");
        assert!(!state.computed(keen).has_keyword(Keyword::Trample));

        // Three distinct types among cards exiled with it — still < 4, no buff.
        exile_typed(&mut state, CardType::Creature);
        exile_typed(&mut state, CardType::Land);
        exile_typed(&mut state, CardType::Artifact);
        assert_eq!(state.computed(keen).power, Some(3), "3 distinct types < 4 → no buff");

        // A fourth distinct type flips the condition on → +4/+4 and trample.
        exile_typed(&mut state, CardType::Enchantment);
        assert_eq!(state.computed(keen).power, Some(7), "≥4 types → +4/+4");
        assert_eq!(state.computed(keen).toughness, Some(7));
        assert!(state.computed(keen).has_keyword(Keyword::Trample), "and has trample");
    }

    #[test]
    fn target_candidates_enforce_type_and_control_filters() {
        use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
        use crate::effects::value::PlayerRef;
        // Earthbend's "target land you control" must offer lands you control — NOT creatures (the
        // Permanent base over all battlefield permanents + an enforced HasCardType filter). And
        // Bushwhack's "creature you don't control" must exclude your own creatures.
        let mut state = cards::build_game(1, &[&[], &[]]);
        let my_forest = put(&mut state, PlayerId(0), grp::FOREST, Zone::Battlefield);
        let _my_bears = put(&mut state, PlayerId(0), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let foe_bears = put(&mut state, PlayerId(1), grp::GRIZZLY_BEARS, Zone::Battlefield);
        let e = pass_engine(state);

        let land_you_control = TargetSpec {
            kind: TargetKind::Permanent(CardFilter::All(vec![
                CardFilter::HasCardType(CardType::Land),
                CardFilter::ControlledBy(PlayerRef::Controller),
            ])),
            min: 1,
            max: 1,
            distinct: true,
        };
        assert_eq!(
            e.target_candidates(&land_you_control, PlayerId(0)),
            vec![Target::Object(my_forest)],
            "earthbend offers only the land you control — not your creatures"
        );

        let creature_you_dont_control = TargetSpec {
            kind: TargetKind::Creature(CardFilter::Not(Box::new(CardFilter::ControlledBy(
                PlayerRef::Controller,
            )))),
            min: 1,
            max: 1,
            distinct: true,
        };
        assert_eq!(
            e.target_candidates(&creature_you_dont_control, PlayerId(0)),
            vec![Target::Object(foe_bears)],
            "Bushwhack's fight target offers only creatures you don't control"
        );
    }

    #[test]
    fn enters_with_counters_equal_to_mana_spent_dyadrine() {
        use crate::basics::{Color, ManaCost};
        use crate::effects::ability::{Ability, ActionPattern, Rewrite};
        use crate::effects::target::CardFilter;
        use crate::effects::value::ValueExpr;
        use std::collections::BTreeMap;
        use std::sync::Arc;
        // Dyadrine's body: "enters with +1/+1 counters equal to the mana spent to cast it." A 0/0
        // cast for {2}{G} (3 mana) enters as a 3/3 via EntersWithCountersValue{ ManaSpent }.
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Dyadrine (test)".into(),
                card_types: vec![CardType::Creature],
                colors: vec![Color::Green],
                mana_cost: Some(ManaCost {
                    generic: 2,
                    colored: BTreeMap::from([(Color::Green, 1)]),
                    x: 0,
                }),
                power: Some(0),
                toughness: Some(0),
                grp_id: 9800,
                ..Default::default()
            },
            abilities: vec![Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersWithCountersValue {
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::ManaSpent,
                },
            }],
            text: String::new(),
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let card = {
            let c = state.card_db().get(9800).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        for _ in 0..3 {
            let f = state.card_db().get(grp::FOREST).unwrap().chars.clone();
            state.add_card(PlayerId(0), f, Zone::Battlefield); // pays {2}{G}
        }
        let mut e = pass_engine(state);
        e.cast_spell(PlayerId(0), card, CastVariant::Normal);
        e.resolve_top(); // resolves onto the battlefield → ETB enters-with-counters
        e.run_agenda();
        assert_eq!(
            e.state.object(card).counters.get(&CounterKind::PlusOnePlusOne),
            3,
            "entered with counters equal to the 3 mana spent"
        );
        let cc = e.state.computed(card);
        assert_eq!(cc.power, Some(3), "0/0 + 3 counters = 3/3");
        assert_eq!(cc.toughness, Some(3));
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
            text: String::new(),
            ..Default::default()
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
                subtypes: vec![crate::subtypes::CreatureType::Bird.into()],
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
            text: String::new(),
            ..Default::default()
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
            text: String::new(),
            ..Default::default()
        });

        // 0/2 that prevents combat damage to itself (Fog Bank stand-in).
        db.insert(CardDef {
            chars: Characteristics {
                name: "Test Fog".into(),
                card_types: vec![CardType::Creature],
                subtypes: vec![crate::subtypes::CreatureType::Wall.into()],
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
            text: String::new(),
            ..Default::default()
        });

        // 0/0 that enters with a +1/+1 counter ({G}); drives the replacement-pass tests.
        db.insert(CardDef {
            chars: Characteristics {
                name: "Test Scaler".into(),
                card_types: vec![CardType::Creature],
                subtypes: vec![], // synthetic stand-in; subtype irrelevant to the replacement-pass test
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
            text: String::new(),
            ..Default::default()
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
                subtypes: vec![crate::subtypes::EnchantmentType::Aura.into()],
                colors: vec![Color::Green],
                mana_cost: Some(cards::mana_cost(0, &[(Color::Green, 1)])),
                grp_id: synth::TRAMPLE_AURA,
                ..Default::default()
            },
            abilities: vec![
                host(StaticContribution::ModifyPT { power: 2, toughness: 0 }),
                host(StaticContribution::GrantKeyword(Keyword::Trample)),
            ],
            text: String::new(),
            ..Default::default()
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

    /// A land (grp 9300) whose ETB carries `rewrite`, in P0's hand, + a custom db. Agent[0] = `a0`.
    fn dual_land_in_hand(rewrite: crate::effects::ability::Rewrite, a0: Box<dyn Agent>) -> (Engine, ObjId) {
        use crate::effects::ability::{Ability, ActionPattern};
        use crate::effects::target::CardFilter;
        use crate::state::Characteristics;
        use std::sync::Arc;
        let mut db = cards::starter_db();
        db.insert(cards::CardDef {
            chars: Characteristics {
                name: "Test Dual".into(),
                card_types: vec![CardType::Land],
                grp_id: 9300,
                ..Default::default()
            },
            abilities: vec![Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite,
            }],
            ..Default::default()
        });
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(db));
        let chars = state.card_db().get(9300).unwrap().chars.clone();
        let land = state.add_card(PlayerId(0), chars, Zone::Hand);
        (Engine::new(state, vec![a0, Box::new(PassAgent)]), land)
    }

    #[test]
    fn check_land_enters_tapped_unless_you_control_a_basic() {
        // C11 check-land flavor: `EntersTappedUnless(control a basic land)` — no choice.
        use crate::effects::ability::Rewrite;
        use crate::effects::condition::Condition;
        use crate::effects::target::CardFilter;
        use crate::effects::value::{PlayerRef, ValueExpr};
        use crate::state::Characteristics;
        let rw = Rewrite::EntersTappedUnless(Condition::CountAtLeast {
            zone: Zone::Battlefield,
            filter: CardFilter::All(vec![
                CardFilter::HasCardType(CardType::Land),
                CardFilter::Supertype(crate::subtypes::Supertype::Basic),
            ]),
            controller: Some(PlayerRef::Controller),
            n: ValueExpr::Fixed(1),
        });
        // No basic controlled → enters tapped.
        let (mut e, land) = dual_land_in_hand(rw.clone(), Box::new(PassAgent));
        e.play_land(PlayerId(0), land);
        assert!(e.state.object(land).status.tapped, "no basic → enters tapped");
        // Control a basic → enters untapped.
        let (mut e, land) = dual_land_in_hand(rw, Box::new(PassAgent));
        e.state.add_card(PlayerId(0), Characteristics::basic_land("Forest"), Zone::Battlefield);
        e.play_land(PlayerId(0), land);
        assert!(!e.state.object(land).status.tapped, "control a basic → enters untapped");
    }

    #[test]
    fn shock_land_pays_life_or_enters_tapped() {
        // C11 shock-land flavor: `EntersTappedUnlessPay{life:2}` — controller chooses as it enters.
        use crate::effects::ability::Rewrite;
        let rw = Rewrite::EntersTappedUnlessPay { life: 2 };
        // Pay 2 life (ConfirmYesAgent) → untapped, 2 life lost.
        let (mut e, land) = dual_land_in_hand(rw.clone(), Box::new(ConfirmYesAgent));
        let life_before = e.state.player(PlayerId(0)).life;
        e.play_land(PlayerId(0), land);
        assert!(!e.state.object(land).status.tapped, "paid → enters untapped");
        assert_eq!(e.state.player(PlayerId(0)).life, life_before - 2, "paid 2 life");
        // Decline (PassAgent → Pass) → tapped, no life lost.
        let (mut e, land) = dual_land_in_hand(rw, Box::new(PassAgent));
        let life_before = e.state.player(PlayerId(0)).life;
        e.play_land(PlayerId(0), land);
        assert!(e.state.object(land).status.tapped, "declined → enters tapped");
        assert_eq!(e.state.player(PlayerId(0)).life, life_before, "no life paid");
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

    /// ROOT FIX (CR 601.2c): an Aura is not offered as a Cast when it has no legal permanent to
    /// enchant — the case that hung the web UI (Pacifism on an empty board → a `ChooseTargets` with
    /// zero candidates for a required slot, no legal response, deadlock). With a creature on the
    /// board the Cast returns. Auras have no spell ability, so their enchant target is structural;
    /// the offer-side gate (`card_castable_targets`) now accounts for it, not just spell-effect
    /// targets.
    #[test]
    fn aura_not_offered_without_a_legal_enchant_target() {
        use crate::cards::{grp, starter_db};
        use crate::state::GameState;
        use std::sync::Arc;

        // P0 in precombat main with Pacifism ({1}{W}) in hand and two Plains to pay for it — but no
        // creature anywhere to enchant.
        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        for _ in 0..2 {
            let c = state.card_db().get(grp::PLAINS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Battlefield);
        }
        let pacifism = {
            let c = state.card_db().get(grp::PACIFISM).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        state.phase = Phase::PrecombatMain;
        state.active_player = PlayerId(0);
        let mut engine = pass_engine(state);

        let offers_pacifism = |e: &Engine| {
            e.legal_priority_actions(PlayerId(0))
                .iter()
                .any(|a| matches!(a, PlayableAction::Cast { spell, .. } if *spell == pacifism))
        };
        // Empty board (no creatures): the Aura is NOT castable — no legal enchant target (601.2c).
        assert!(!offers_pacifism(&engine), "Pacifism must not be offered with no creature to enchant");

        // Add a creature: now there IS a legal enchant target, so the Cast is offered.
        let bears = engine.state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        engine.state.add_card(PlayerId(1), bears, Zone::Battlefield);
        assert!(offers_pacifism(&engine), "with a creature on the board the Aura becomes castable");
    }

    /// Regression (CR 601.2c): a creature-only-target spell (Murder — "Destroy target creature") has
    /// no legal target with no creatures in play, so it isn't castable; adding a creature flips it.
    /// Burn spells ("any target") can't show the bug because the player is always a legal target —
    /// this uses a creature-restricted target. Checks `card_castable_targets` directly (the exact
    /// offer-side gate) so mana affordability, which is gated separately, doesn't confound it.
    #[test]
    fn creature_target_spell_castability_tracks_creatures() {
        use crate::cards::{grp, starter_db};
        use crate::state::GameState;
        use std::sync::Arc;

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let murder = {
            let c = state.card_db().get(grp::MURDER).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        state.active_player = PlayerId(0);
        let mut engine = pass_engine(state);

        // No creatures ⇒ no legal target ⇒ not castable (target-wise).
        assert!(!engine.card_castable_targets(murder, PlayerId(0)));
        // Add a creature ⇒ a legal target exists ⇒ castable (target-wise).
        let bears = engine.state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
        engine.state.add_card(PlayerId(1), bears, Zone::Battlefield);
        assert!(engine.card_castable_targets(murder, PlayerId(0)));
    }

    /// DEFENSIVE UNHANG (CR 601.2c / 728): if a cast reaches the target step with a required slot
    /// that has no legal candidate, the engine rewinds instead of deadlocking — the card returns to
    /// its owner's hand and the stack is left empty (no mana was paid; targets precede costs at
    /// 601.2f). Driven by calling `cast_spell` directly on Pacifism with an empty board, bypassing
    /// the offer gate that normally prevents ever reaching here.
    #[test]
    fn cast_with_no_legal_target_rewinds_instead_of_hanging() {
        use crate::cards::{grp, starter_db};
        use crate::state::GameState;
        use std::sync::Arc;

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        let pacifism = {
            let c = state.card_db().get(grp::PACIFISM).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        state.active_player = PlayerId(0);
        let mut engine = pass_engine(state);

        engine.cast_spell(PlayerId(0), pacifism, CastVariant::Normal);

        assert!(engine.state.stack.items.is_empty(), "the aborted cast left nothing on the stack");
        assert!(
            engine.state.player(PlayerId(0)).hand.contains(&pacifism),
            "the card was returned to its owner's hand"
        );
        assert_eq!(engine.state.object(pacifism).zone, Zone::Hand);
    }

    /// The `ask`-seam invariant helper itself: a required target slot with no candidate has no legal
    /// response; an optional slot (min=0) always does (choose nothing). This is what the debug
    /// assertion in `ask` enforces on every emitted decision.
    #[test]
    fn request_has_legal_response_flags_unsatisfiable_required_slot() {
        let slot = |legal: Vec<Target>, min: u32| TargetSlot {
            description: String::new(),
            legal,
            min,
            max: 1,
        };
        let make = |min: u32| DecisionRequest::ChooseTargets {
            for_action: ActionRef(StackId(0)),
            source: None,
            slots: vec![slot(vec![], min)],
        };
        assert!(!request_has_legal_response(&make(1)), "required slot, zero candidates ⇒ no response");
        assert!(request_has_legal_response(&make(0)), "optional slot ⇒ 'choose nothing' is legal");
    }

    /// #36: manual mana. A seat with `manual_mana` ON is offered an `ActivateMana` per untapped
    /// source so a human can tap SPECIFIC lands; the floated mana is then spent by a later cast,
    /// so no OTHER source is auto-tapped (source control). A default (agent/replay) seat never sees
    /// these — mana abilities stay out of the action space and it auto-pays.
    #[test]
    fn manual_mana_lets_a_seat_tap_chosen_sources_then_cast_from_float() {
        use crate::basics::Color;
        use crate::cards::{grp, starter_db};
        use std::sync::Arc;

        let mut state = GameState::new(2, 1);
        state.set_card_db(Arc::new(starter_db()));
        // P0 in precombat main: three Forests on the battlefield + a Grizzly Bears ({1}{G}) in hand.
        let forests: Vec<ObjId> = (0..3)
            .map(|_| {
                let c = state.card_db().get(grp::FOREST).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Battlefield)
            })
            .collect();
        let bears = {
            let c = state.card_db().get(grp::GRIZZLY_BEARS).unwrap().chars.clone();
            state.add_card(PlayerId(0), c, Zone::Hand)
        };
        state.phase = Phase::PrecombatMain;
        state.active_player = PlayerId(0);
        let mut engine = pass_engine(state);

        // Default seat (agent/replay): NO mana abilities in the action space — just the cast.
        let off = engine.legal_priority_actions(PlayerId(0));
        assert!(
            !off.iter().any(|a| matches!(a, PlayableAction::ActivateMana { .. })),
            "manual mana OFF (default) keeps ActivateMana out of the action space: {off:?}"
        );
        assert!(
            off.iter().any(|a| matches!(a, PlayableAction::Cast { .. })),
            "the {{1}}{{G}} spell is castable"
        );

        // Turn on manual mana (a UI session): one ActivateMana per untapped Forest, in order.
        engine.set_manual_mana(PlayerId(0), true);
        let on = engine.legal_priority_actions(PlayerId(0));
        let mana_sources: Vec<ObjId> = on
            .iter()
            .filter_map(|a| match a {
                PlayableAction::ActivateMana { source, .. } => Some(*source),
                _ => None,
            })
            .collect();
        assert_eq!(mana_sources, forests, "one ActivateMana per untapped Forest");

        // Tap the FIRST and THIRD Forests for mana (the player's chosen sources), leaving the middle.
        for &src in &[forests[0], forests[2]] {
            engine.perform_priority_action(
                PlayerId(0),
                &PlayableAction::ActivateMana { source: src, ability: AbilityRef(u32::MAX) },
            );
        }
        let green = |e: &Engine| {
            e.state.player(PlayerId(0)).mana_pool.amounts.get(&Color::Green).copied().unwrap_or(0)
        };
        assert_eq!(green(&engine), 2, "the two chosen Forests floated {{G}}{{G}}");
        assert!(
            engine.state.object(forests[0]).status.tapped
                && engine.state.object(forests[2]).status.tapped,
            "the chosen Forests are tapped"
        );
        assert!(!engine.state.object(forests[1]).status.tapped, "the un-chosen Forest is untouched");

        // Cast the {1}{G} spell: paid from the FLOATING mana (CR 106.4), so the middle Forest is
        // NOT auto-tapped — the human controlled which sources funded the spell.
        engine.cast_spell(PlayerId(0), bears, CastVariant::Normal);
        assert_eq!(green(&engine), 0, "the floated {{G}}{{G}} paid the {{1}}{{G}} cost");
        assert!(
            !engine.state.object(forests[1]).status.tapped,
            "casting consumed the floated mana — the un-chosen Forest stayed untapped (source control)"
        );
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
