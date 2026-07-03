//! The resumable step API (M3 — `docs/design/RESUMABLE_ENGINE.md`).
//!
//! This module hosts the **pull** primitive that inverts the engine's control flow: instead of the
//! game loop blocking inside `Agent::decide`, a [`Session`] runs the game to its next decision and
//! **returns** a [`Step`], then resumes when handed the response. It is the substrate for
//! GIL-free fleet stepping (many games advanced in Rust, Python seeing only batched tensors) and
//! the thin driver the blocking `Agent` trait collapses onto.
//!
//! **How it works.** [`Session`] runs [`EngineCore::run_in_fiber`] inside a stackful coroutine
//! (corosensei). The engine code is untouched: its single `ask` seam (RESUMABLE_ENGINE.md §3.2)
//! checks whether it's running in a fiber and, if so, **suspends** — yielding the decision out —
//! instead of calling an in-core agent. The caller reads the [`Step`], computes a response, and
//! [`submit`](Session::submit)s it; the next [`resume`](Session::resume) feeds it back into the
//! suspended `ask` and the game continues from exactly where it left off (the native call stack
//! *is* the continuation).
//!
//! **Status (M3.2, single-threaded).** This is the working `resume`/`submit` primitive. The core
//! still *holds* its agents (they're simply unused while a `Session` drives it, since `ask`
//! yields); removing them to make `EngineCore: Send` for a rayon/thread-pinned **fleet** is the
//! next step (needs the `Engine` wrapper split). So `Session` is not yet `Send` — one game at a
//! time per thread, which is already enough to drop the gym's per-game OS thread + channels.

use corosensei::stack::DefaultStack;
use corosensei::{Coroutine, CoroutineResult};

use crate::agent::{DecisionRequest, DecisionResponse, PlayerView};
use crate::ids::PlayerId;
use crate::priority::{EngineCore, Outcome};
use crate::state::GameState;

/// Per-fiber stack size. The M3.0 spike measured a 42 KiB worst-case over 125 random games; 256 KiB
/// is ~6× headroom for deeper cascades while keeping a large fleet affordable (RESUMABLE_ENGINE.md
/// §6.5). corosensei installs a guard page, so an overflow faults rather than corrupts.
const FIBER_STACK_BYTES: usize = 256 * 1024;

/// What a [`Session`] yields each time it is advanced (`resume`): either the game reached a player
/// decision and is suspended awaiting a response, or the game is over.
///
/// Mirrors the `resume`/`submit` sketch pre-agreed in `GYM_PLAN.md` §2.2-B, with one addition: the
/// `Decision` variant carries the info-filtered [`PlayerView`] by value. Once suspended, the game
/// state lives inside the coroutine's stack and cannot be borrowed out, so everything a caller
/// needs at the decision point — the seat, its view (for obs encoding / the agent), and the
/// enumerated legal request — must travel in the yield. Building the view is free: `ask` already
/// computes it today.
// `Decision` (a full `PlayerView`) dwarfs `GameOver`, but a `Step` is transient — created, yielded
// across the fiber, and destructured immediately by the driver; never stored in bulk — so the size
// asymmetry costs one short-lived value, not memory pressure. Boxing would only cost the ergonomic
// struct-variant destructuring the driver + tests rely on.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum Step {
    /// The engine reached a choice point for `seat` and is suspended. Feed the chosen
    /// [`DecisionResponse`] back via [`Session::submit`] + [`Session::resume`] to continue.
    Decision {
        seat: PlayerId,
        view: PlayerView,
        request: DecisionRequest,
    },
    /// The game ended (CR 104). The session is finished; `outcome` is the result.
    GameOver { outcome: Outcome },
}

impl Step {
    /// The decision's seat, if this is a `Decision`.
    pub fn seat(&self) -> Option<PlayerId> {
        match self {
            Step::Decision { seat, .. } => Some(*seat),
            Step::GameOver { .. } => None,
        }
    }
    /// Whether the game has ended.
    pub fn is_over(&self) -> bool {
        matches!(self, Step::GameOver { .. })
    }
}

/// A single game as a **resumable** computation: [`resume`](Self::resume) advances it to the next
/// decision (or game-over) and returns a [`Step`]; [`submit`](Self::submit) provides the response
/// the *next* `resume` feeds back in. No `Agent` is consulted — the caller (a blocking driver, or
/// the fleet stepper) supplies every response. See the module docs.
pub struct Session {
    coro: Coroutine<DecisionResponse, Step, EngineCore, DefaultStack>,
    /// The response handed to the next `resume` (via `submit`). The first `resume` has none — its
    /// input is ignored by the fiber body — so a placeholder `Pass` is used to prime it.
    pending: Option<DecisionResponse>,
    /// The finished core, kept for outcome/state inspection once the game is over.
    finished: Option<EngineCore>,
}

impl Session {
    /// Wrap a built [`EngineCore`] as a resumable game. The core's game hasn't started yet; the
    /// first [`resume`](Self::resume) begins it. (The core's own agents are ignored while driven
    /// as a `Session` — `ask` yields instead of consulting them.)
    pub fn start(core: EngineCore) -> Self {
        let stack = DefaultStack::new(FIBER_STACK_BYTES).expect("allocate fiber stack");
        let coro = Coroutine::with_stack(stack, move |yielder, _first: DecisionResponse| {
            core.run_in_fiber(yielder)
        });
        Session { coro, pending: None, finished: None }
    }

    /// Advance the game to its next decision (or to game-over). Returns [`Step::Decision`] while a
    /// player must choose (respond with [`submit`](Self::submit) then call `resume` again), or
    /// [`Step::GameOver`] once the game has ended (idempotent thereafter).
    pub fn resume(&mut self) -> Step {
        if let Some(core) = &self.finished {
            return Step::GameOver { outcome: core.outcome() };
        }
        // The first resume's input is ignored by the fiber body; subsequent ones carry the
        // submitted response back into the suspended `ask`.
        let input = self.pending.take().unwrap_or(DecisionResponse::Pass);
        match self.coro.resume(input) {
            CoroutineResult::Yield(step) => step,
            CoroutineResult::Return(core) => {
                let outcome = core.outcome();
                self.finished = Some(core);
                Step::GameOver { outcome }
            }
        }
    }

    /// Provide the response to the decision the last [`resume`](Self::resume) yielded. The next
    /// `resume` feeds it into the suspended `ask`.
    pub fn submit(&mut self, response: DecisionResponse) {
        self.pending = Some(response);
    }

    /// Whether the game has ended.
    pub fn is_over(&self) -> bool {
        self.finished.is_some()
    }

    /// The final game state, once the game is over (`None` while still in progress — the state
    /// lives inside the suspended fiber and can't be borrowed out mid-game).
    pub fn state(&self) -> Option<&GameState> {
        self.finished.as_ref().map(|c| &c.state)
    }

    /// The omniscient [`Replay`](crate::replay::Replay) recorded during the game — `Some` once the
    /// game is over. The frames are populated only if the core was built with `record_replay(true)`
    /// before [`Session::start`] (the gym's training-replay export path); mirrors `Engine::replay`
    /// for a Session-driven game.
    pub fn replay(&self) -> Option<crate::replay::Replay> {
        self.finished.as_ref().map(|c| c.replay())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{Agent, RandomAgent};
    use crate::cards::{self, preset_deck};
    use crate::priority::Engine;

    /// Driving a game through `resume`/`submit` (answering each decision externally) produces the
    /// **same outcome** as the blocking `run_game` with identically-seeded agents — i.e. the
    /// control-flow inversion is behaviour-preserving. Same engine + same game RNG ⇒ the decision
    /// points and their per-seat order are identical, so per-seat `RandomAgent`s with the same
    /// seeds answer identically.
    #[test]
    fn session_drive_matches_blocking_run() {
        let seeds = [1u64, 2, 7, 42];
        for seed in seeds {
            // Blocking reference.
            let blocking = {
                let state = cards::build_game(seed, &[&preset_deck("bears").unwrap(), &preset_deck("heralds").unwrap()]);
                let mut e = Engine::new(
                    state,
                    vec![Box::new(RandomAgent::new(seed)), Box::new(RandomAgent::new(seed ^ 0x5))],
                );
                e.run_game();
                e.outcome()
            };

            // Same game, driven via the Session primitive with external per-seat RandomAgents.
            let session_outcome = {
                let state = cards::build_game(seed, &[&preset_deck("bears").unwrap(), &preset_deck("heralds").unwrap()]);
                // The core's own agents are ignored (ask yields); pass placeholders.
                let core = Engine::new(
                    state,
                    vec![Box::new(RandomAgent::new(0)), Box::new(RandomAgent::new(0))],
                );
                let mut sess = Session::start(core);
                let mut ext: Vec<RandomAgent> =
                    vec![RandomAgent::new(seed), RandomAgent::new(seed ^ 0x5)];
                loop {
                    match sess.resume() {
                        Step::Decision { seat, view, request } => {
                            let resp = ext[seat.0 as usize].decide(&view, &request);
                            sess.submit(resp);
                        }
                        Step::GameOver { outcome } => break outcome,
                    }
                }
            };

            assert_eq!(
                (blocking.winner, blocking.turns, blocking.reason),
                (session_outcome.winner, session_outcome.turns, session_outcome.reason),
                "Session drive diverged from the blocking run at seed {seed}",
            );
            assert!(session_outcome.turns > 0, "the driven game actually played");
        }
    }

    /// A minimal smoke test that `resume` yields real decisions and terminates.
    #[test]
    fn session_yields_decisions_then_game_over() {
        let state = cards::build_game(3, &[&preset_deck("burn").unwrap(), &preset_deck("burn").unwrap()]);
        let core = Engine::new(state, vec![Box::new(RandomAgent::new(1)), Box::new(RandomAgent::new(2))]);
        let mut sess = Session::start(core);
        let mut decisions = 0u32;
        let mut ext = [RandomAgent::new(1), RandomAgent::new(2)];
        let outcome = loop {
            match sess.resume() {
                Step::Decision { seat, view, request } => {
                    decisions += 1;
                    let resp = ext[seat.0 as usize].decide(&view, &request);
                    sess.submit(resp);
                    assert!(decisions < 1_000_000, "runaway");
                }
                Step::GameOver { outcome } => break outcome,
            }
        };
        assert!(decisions > 0, "the game asked at least one decision");
        assert!(outcome.turns > 0);
        assert!(sess.is_over());
        assert!(sess.state().is_some(), "finished state is inspectable");
    }

    /// A `record_replay(true)` core driven through a Session yields its omniscient replay — the
    /// gym's training-replay export path (records inside the fiber, extracted after game-over).
    #[test]
    fn session_records_a_replay_when_enabled() {
        let state = cards::build_game(5, &[&preset_deck("bears").unwrap(), &preset_deck("bears").unwrap()]);
        let mut core = Engine::new(state, vec![Box::new(RandomAgent::new(1)), Box::new(RandomAgent::new(2))]);
        core.record_replay(true);
        let mut sess = Session::start(core);
        let mut ext = [RandomAgent::new(1), RandomAgent::new(2)];
        loop {
            match sess.resume() {
                Step::Decision { seat, view, request } => {
                    sess.submit(ext[seat.0 as usize].decide(&view, &request));
                }
                Step::GameOver { .. } => break,
            }
        }
        let replay = sess.replay().expect("a finished session exposes its replay");
        assert!(replay.frames.len() > 1, "frames were recorded through the fiber");
        assert!(replay.meta.result.is_some(), "the finished replay has its result stamped");
    }
}
