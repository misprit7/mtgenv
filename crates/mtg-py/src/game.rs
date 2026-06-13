//! The thread + channel bridge that inverts the engine's push-based control flow into the
//! pull-based shape a Gym `step` wants — a direct port of `mtg-gre-server`'s `GreSessionAgent`
//! pattern (GYM_PLAN §2.2 approach A), but talking to an in-process Python policy instead of a
//! WebSocket client, and with **no engine changes**.
//!
//! `mtg-core` is synchronous: `Engine::run_game` drives, calling each seat's `Agent::decide`
//! whenever that seat must choose. We run the whole game on its own OS thread; every seat is a
//! [`PyAgent`] that, on `decide`, ships `(seat, view, request)` to the Python side over a channel
//! and **blocks** until a `DecisionResponse` comes back. The Python side pulls the request
//! (`step_to_decision`) and pushes the answer (`apply`), unblocking the game thread. The thread is
//! idle while Python thinks; throughput comes from running many games (GYM_PLAN §5–6).
//!
//! Both seats are `PyAgent`s sharing one request channel and one (mutex-wrapped) response channel.
//! The engine calls `decide` strictly sequentially, so exactly one agent ever waits at a time —
//! the mutex is never contended, and a response always belongs to the one pending decision.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use mtg_core::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView, RandomAgent};
use mtg_core::basics::Zone;
use mtg_core::ids::PlayerId;
use mtg_core::priority::{EndReason, Engine};
use mtg_core::replay::{Replay, ReplaySource};
use mtg_core::state::{Characteristics, GameState};

/// Which tiny built-in deck/matchup a game uses. The card pool grows in later milestones; for
/// milestone 0 these three exercise lands-only (deck-out), casting + the stack + combat (demo),
/// and a burn-vs-creatures race.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deck {
    LandsOnly,
    Demo,
    BurnVsBears,
}

impl Deck {
    pub fn parse(s: &str) -> Option<Deck> {
        match s.to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
            "lands" | "lands_only" | "landsonly" => Some(Deck::LandsOnly),
            "demo" => Some(Deck::Demo),
            "burn_vs_bears" | "burnvsbears" | "bvb" => Some(Deck::BurnVsBears),
            _ => None,
        }
    }

    fn build(self, seed: u64) -> GameState {
        match self {
            Deck::LandsOnly => lands_only_state(2, seed),
            Deck::Demo => mtg_core::cards::two_player_demo_game(seed),
            Deck::BurnVsBears => mtg_core::cards::burn_vs_bears_game(seed),
        }
    }
}

const BASICS: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];
const LIBRARY_SIZE: usize = 14;

/// A lands-only state: `num_players` seats each with a round-robin basic-land library (small so
/// the game decks out quickly). Replicated here (it's three lines) so this crate depends only on
/// `mtg-core`, never on `mtg-gre-server` where the human-play variant lives.
fn lands_only_state(num_players: usize, seed: u64) -> GameState {
    let mut state = GameState::new(num_players, seed);
    for seat in 0..num_players as u32 {
        for i in 0..LIBRARY_SIZE {
            state.add_card(
                PlayerId(seat),
                Characteristics::basic_land(BASICS[i % BASICS.len()]),
                Zone::Library,
            );
        }
    }
    state
}

/// A message from the game thread to the Python side: either a decision to answer, or the game
/// finished (with the conservation/outcome summary computed on the thread that owns the state, plus
/// the recorded [`Replay`] when recording was enabled).
pub enum FromGame {
    Decision {
        seat: PlayerId,
        view: PlayerView,
        req: DecisionRequest,
    },
    GameOver {
        summary: EndSummary,
        /// The omniscient replay (`Some` iff the game ran with `record_replay`), with `created_at`
        /// / player names+decks left for the caller to stamp (the core has no clock).
        replay: Option<Replay>,
    },
}

/// The terminal summary, computed in the game thread (it owns the final `GameState`). Carries the
/// conservation invariants so the Python smoke test can assert them without reaching into Rust.
#[derive(Clone, Copy, Debug)]
pub struct EndSummary {
    /// Winning seat index, or `None` for a draw / turn-cap.
    pub winner: Option<u32>,
    pub turns: u32,
    pub reason: &'static str,
    /// `objects.len()` at game start and end — equal iff no card was created/destroyed (the tiny
    /// pool has no tokens/copies, so object count is conserved).
    pub initial_object_count: usize,
    pub object_count: usize,
    /// Sum of every zone's size at game end (incl. the stack). Equals `object_count` iff every
    /// card is accounted for in exactly one zone.
    pub zone_sum: usize,
}

fn reason_str(r: EndReason) -> &'static str {
    match r {
        EndReason::ZeroLife => "zero_life",
        EndReason::Decked => "decked",
        EndReason::Poison => "poison",
        EndReason::DrawOrCapped => "draw_or_capped",
    }
}

fn end_summary(engine: &Engine, initial_object_count: usize) -> EndSummary {
    let outcome = engine.outcome();
    let st = &engine.state;
    let zone_sum: usize = st
        .players
        .iter()
        .map(|p| {
            p.library.len() + p.hand.len() + p.battlefield.len() + p.graveyard.len() + p.exile.len()
        })
        .sum::<usize>()
        + st.stack.len();
    EndSummary {
        winner: outcome.winner.map(|w| w.0),
        turns: outcome.turns,
        reason: reason_str(outcome.reason),
        initial_object_count,
        object_count: st.objects.len(),
        zone_sum,
    }
}

/// One seat, answered by the Python policy over the channels. On `decide` it ships the request and
/// blocks for the response; if the Python side has gone away (env reset/dropped), it falls back to
/// a `RandomAgent` so the game thread always terminates cleanly instead of hanging — exactly the
/// `GreSessionAgent` disconnect behaviour.
struct PyAgent {
    seat: PlayerId,
    to_py: Sender<FromGame>,
    from_py: Arc<Mutex<Receiver<DecisionResponse>>>,
    fallback: RandomAgent,
}

impl Agent for PyAgent {
    fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
        let sent = self.to_py.send(FromGame::Decision {
            seat: self.seat,
            view: view.clone(),
            req: req.clone(),
        });
        if sent.is_err() {
            return self.fallback.decide(view, req);
        }
        // Block the game thread until the matching response arrives. Sequential `decide` calls
        // mean only this agent waits now, so the mutex is uncontended and the answer is ours.
        let guard = match self.from_py.lock() {
            Ok(g) => g,
            Err(_) => return self.fallback.decide(view, req),
        };
        match guard.recv() {
            Ok(resp) => resp,
            Err(_) => self.fallback.decide(view, req),
        }
    }
}

/// Owns the running game thread and the response-channel end. Dropping it tears the game down
/// (the fallback agent finishes it) and joins the thread, so no thread is ever leaked.
///
/// The *request* receiver (`from_game`) is returned separately by [`spawn`](GameConn::spawn) and
/// owned by the caller, because the caller blocks on it with the GIL released — and `std`'s
/// `Receiver` is `Send` but not `Sync`, so it must be *moved* into the blocking closure, not
/// borrowed.
pub struct GameConn {
    to_game: Option<Sender<DecisionResponse>>,
    handle: Option<JoinHandle<()>>,
}

impl GameConn {
    /// Spawn a fresh game on its own thread. Both seats are `PyAgent`s feeding the returned
    /// `Receiver`; the caller pulls decisions from it and answers via [`respond`](GameConn::respond).
    /// With `record_replay`, the engine records an omniscient [`Replay`] tagged
    /// `AiTraining { step: replay_step }`, shipped in `GameOver` at the end.
    pub fn spawn(
        deck: Deck,
        seed: u64,
        auto_pass: bool,
        record_replay: bool,
        replay_step: u64,
    ) -> (GameConn, Receiver<FromGame>) {
        let (to_py, from_game) = mpsc::channel::<FromGame>();
        let (to_game, resp_rx) = mpsc::channel::<DecisionResponse>();
        let from_py = Arc::new(Mutex::new(resp_rx));

        let handle = std::thread::spawn(move || {
            let state = deck.build(seed);
            let initial_object_count = state.objects.len();
            let n = state.players.len() as u32;
            let agents: Vec<Box<dyn Agent>> = (0..n)
                .map(|s| {
                    Box::new(PyAgent {
                        seat: PlayerId(s),
                        to_py: to_py.clone(),
                        from_py: Arc::clone(&from_py),
                        // A per-(seat,seed) fallback so a torn-down game still finishes legally.
                        fallback: RandomAgent::new(
                            0xC0FFEE ^ seed.wrapping_mul(0x9E3779B1).wrapping_add(s as u64),
                        ),
                    }) as Box<dyn Agent>
                })
                .collect();
            let mut engine = Engine::new(state, agents);
            if auto_pass {
                engine.set_arena_auto_pass(true);
            }
            if record_replay {
                engine.set_replay_source(ReplaySource::AiTraining { step: replay_step });
                engine.record_replay(true);
            }
            engine.run_game();
            let summary = end_summary(&engine, initial_object_count);
            let replay = record_replay.then(|| engine.replay());
            // Best-effort: the receiver is gone if the env already moved on.
            let _ = to_py.send(FromGame::GameOver { summary, replay });
        });

        (
            GameConn {
                to_game: Some(to_game),
                handle: Some(handle),
            },
            from_game,
        )
    }

    /// Send the decoded response to the waiting game thread. Returns `false` if the game thread
    /// has already exited (channel closed).
    pub fn respond(&self, resp: DecisionResponse) -> bool {
        match &self.to_game {
            Some(tx) => tx.send(resp).is_ok(),
            None => false,
        }
    }
}

impl Drop for GameConn {
    fn drop(&mut self) {
        // Drop the response sender FIRST: any agent blocked in `recv` then gets `Err` and falls
        // back to its `RandomAgent`, which plays the game out without the channel — so the join
        // below returns promptly instead of deadlocking on a game that's mid-decision.
        self.to_game.take();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_parse_accepts_aliases() {
        assert_eq!(Deck::parse("lands"), Some(Deck::LandsOnly));
        assert_eq!(Deck::parse("Demo"), Some(Deck::Demo));
        assert_eq!(Deck::parse("burn-vs-bears"), Some(Deck::BurnVsBears));
        assert_eq!(Deck::parse("nope"), None);
    }

    // A full game driven entirely through the channel bridge by an in-thread "policy" that picks
    // the first legal option every time — proves the spawn/respond/teardown loop end to end
    // without Python. (The Python smoke test does the randomized, thousands-of-games version.)
    #[test]
    fn channel_bridge_plays_a_game_to_completion() {
        let (conn, from_game) = GameConn::spawn(Deck::LandsOnly, 7, true, false, 0);
        let mut decisions = 0usize;
        let summary = loop {
            match from_game.recv().expect("game thread alive") {
                FromGame::Decision { view, req, .. } => {
                    // Drive the factored sub-steps to a commit by always taking the first legal slot.
                    let mut inter = crate::codec::Interaction::new(&view, &req);
                    let resp = loop {
                        decisions += 1;
                        let mask = inter.mask();
                        let slot = mask.iter().position(|b| *b).expect("non-empty mask");
                        if let Some(r) = inter.apply(slot) {
                            break r;
                        }
                    };
                    assert!(conn.respond(resp));
                }
                FromGame::GameOver { summary, replay } => {
                    assert!(replay.is_none(), "no replay unless record_replay");
                    break summary;
                }
            }
        };
        assert!(decisions > 0, "a real game has decisions");
        assert_eq!(
            summary.object_count, summary.initial_object_count,
            "card conservation"
        );
        assert_eq!(summary.zone_sum, summary.object_count, "zone conservation");
    }

    // record_replay=true ⇒ GameOver carries a Replay with frames, the AiTraining source, and the
    // engine-filled result. created_at stays 0 (caller stamps it) — validates the a533720 schema.
    #[test]
    fn records_replay_when_enabled() {
        let (conn, from_game) = GameConn::spawn(Deck::LandsOnly, 3, true, true, 1234);
        let replay = loop {
            match from_game.recv().expect("game thread alive") {
                FromGame::Decision { view, req, .. } => {
                    let mut inter = crate::codec::Interaction::new(&view, &req);
                    let resp = loop {
                        let slot = inter.mask().iter().position(|b| *b).expect("non-empty mask");
                        if let Some(r) = inter.apply(slot) {
                            break r;
                        }
                    };
                    conn.respond(resp);
                }
                FromGame::GameOver { replay, .. } => break replay.expect("replay recorded"),
            }
        };
        assert!(replay.frames.len() > 1, "replay has frames");
        assert_eq!(replay.frames[0].label, "game start");
        assert_eq!(replay.meta.source, ReplaySource::AiTraining { step: 1234 });
        assert!(replay.meta.result.is_some(), "engine fills result at game end");
        assert_eq!(replay.meta.created_at, 0, "caller stamps the clock");
    }
}
