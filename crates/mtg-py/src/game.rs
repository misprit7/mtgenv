//! The **pull** bridge from the engine to the Python side, built on `mtg_core::session::Session`
//! (the resumable step API, M3). `mtg-core` is push-based — `Engine::run_game` drives, calling each
//! seat's `Agent::decide` — so this used to run the whole game on its own OS thread and answer each
//! `decide` over a channel (a `GreSessionAgent`-style bridge). `Session` inverts that natively: it
//! runs the game inside a stackful fiber whose single `ask` seam **suspends** at every decision and
//! **resumes** when handed the response. So `PyGame` drives the game directly on the calling thread
//! — no per-game OS thread, mpsc channels, or `PyAgent`. The Python side pulls the request
//! (`step_to_decision` → `Session::resume`) and pushes the answer (`apply` → `Session::submit`).

use mtg_core::agent::{Agent, RandomAgent};
use mtg_core::basics::Zone;
use mtg_core::ids::PlayerId;
use mtg_core::priority::{EndReason, Engine, Outcome};
use mtg_core::replay::ReplaySource;
use mtg_core::session::Session;
use mtg_core::state::{Characteristics, GameState};

/// Which built-in deck/matchup a game uses. `LandsOnly`/`Demo`/`BurnVsBears` are the tiny milestone-0
/// pools (deck-out, casting+stack+combat, a burn-vs-creatures race); `Selesnya` is the M4 mirror on
/// the implemented landfall pool — a real card pool to test that the policy's `grp_id` embedding
/// generalizes past the 3-card demo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deck {
    LandsOnly,
    Demo,
    BurnVsBears,
    Selesnya,
    /// "Heralds": an intentionally degenerate RL sanity mirror — 40 Mist-Cloaked Herald ({U} 1/1,
    /// can't be blocked) + 20 Island. Optimal play is trivial (always play a land, cast every
    /// Herald, attack with everything), so a converging policy should drive playland/cast/attack
    /// rates → ~1.0. Used to verify training actually learns.
    Heralds,
    /// "Bears": the next sanity tier up from Heralds — 40 Grizzly Bears ({1}{G} 2/2 vanilla) + 20
    /// Forest. Unlike the Heralds race, bears CAN block, so the mirror has real combat judgment
    /// (attacking into untapped 2/2s trades; board stalls are plausible) and there's no provable
    /// "always attack" optimum — a test that learning progresses sensibly with combat credit.
    Bears,
    /// "Swine": tier-3 combat-judgment mirror — 25 Forest + 10 Argothian Swine ({3}{G} 3/3 TRAMPLE)
    /// + 25 Grizzly Bears. Trample makes single-blocking a Swine strictly bad (chump-block leaks
    /// damage AND the blocker dies), so — unlike the vanilla Bears mirror — a policy with real
    /// combat judgment should decline those blocks (aggregate block_rate settles below 1.0).
    Swine,
}

impl Deck {
    pub fn parse(s: &str) -> Option<Deck> {
        match s.to_ascii_lowercase().replace(['-', ' '], "_").as_str() {
            "lands" | "lands_only" | "landsonly" => Some(Deck::LandsOnly),
            "demo" => Some(Deck::Demo),
            "burn_vs_bears" | "burnvsbears" | "bvb" => Some(Deck::BurnVsBears),
            "selesnya" | "landfall" => Some(Deck::Selesnya),
            "heralds" => Some(Deck::Heralds),
            "bears" => Some(Deck::Bears),
            "swine" => Some(Deck::Swine),
            _ => None,
        }
    }

    fn build(self, seed: u64) -> GameState {
        match self {
            Deck::LandsOnly => lands_only_state(2, seed),
            Deck::Demo => mtg_core::cards::two_player_demo_game(seed),
            Deck::BurnVsBears => mtg_core::cards::burn_vs_bears_game(seed),
            Deck::Selesnya => {
                // Mirror of the engine's Selesnya landfall preset (one library per seat).
                let d = mtg_core::cards::preset_deck("selesnya").expect("selesnya preset deck");
                mtg_core::cards::build_game(seed, &[d.as_slice(), d.as_slice()])
            }
            Deck::Heralds => {
                // Mirror of the degenerate "heralds" sanity preset (one library per seat).
                let d = mtg_core::cards::preset_deck("heralds").expect("heralds preset deck");
                mtg_core::cards::build_game(seed, &[d.as_slice(), d.as_slice()])
            }
            Deck::Bears => {
                // Mirror of the "bears" sanity preset (one library per seat) — combat-capable.
                let d = mtg_core::cards::preset_deck("bears").expect("bears preset deck");
                mtg_core::cards::build_game(seed, &[d.as_slice(), d.as_slice()])
            }
            Deck::Swine => {
                // Mirror of the "swine" tier-3 preset (one library per seat) — trample combat judgment.
                let d = mtg_core::cards::preset_deck("swine").expect("swine preset deck");
                mtg_core::cards::build_game(seed, &[d.as_slice(), d.as_slice()])
            }
        }
    }

    /// The card-identity **vocabulary** for this matchup: the sorted unique `grp_id`s across BOTH
    /// seats' decks (the union). The Python obs layer turns this into a fixed one-hot per card row
    /// (plus a token reserve), so the policy sees *explicit* card identity rather than only the
    /// hashed `grp_id` embedding. Deterministic + identical for every env of a deck, so the one-hot
    /// index space is stable across a training run.
    pub fn vocab(self) -> Vec<u32> {
        use mtg_core::cards::preset_deck;
        let lists: Vec<Vec<u32>> = match self {
            Deck::Demo => vec![preset_deck("demo").unwrap_or_default()],
            Deck::BurnVsBears => vec![
                preset_deck("burn").unwrap_or_default(),
                preset_deck("bears").unwrap_or_default(),
            ],
            Deck::Selesnya => vec![preset_deck("selesnya").unwrap_or_default()],
            Deck::Heralds => vec![preset_deck("heralds").unwrap_or_default()],
            Deck::Bears => vec![preset_deck("bears").unwrap_or_default()],
            Deck::Swine => vec![preset_deck("swine").unwrap_or_default()],
            Deck::LandsOnly => vec![], // basics-only deck-out test; no spell pool to identify
        };
        let mut set = std::collections::BTreeSet::new();
        for l in lists {
            for g in l {
                set.insert(g);
            }
        }
        set.into_iter().collect()
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

/// The terminal summary, computed once the game is over (from the finished state). Carries the
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

/// The terminal summary from the game's [`Outcome`] and its finished [`GameState`]. (Previously read
/// off an `&Engine`; a `Session` yields the outcome and exposes the final state, so the same numbers
/// are assembled from those — `initial_object_count` is captured at build time in [`start_session`].)
pub fn end_summary_from(outcome: &Outcome, st: &GameState, initial_object_count: usize) -> EndSummary {
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

/// Build a fresh game of `deck` at `seed` as a resumable [`Session`], configured for the Arena
/// auto-pass profile and (optionally) omniscient replay recording. Returns the session + the object
/// count at game start (for the conservation check in [`end_summary_from`]). The core's own agents
/// are placeholders — a `Session` never consults them (its `ask` seam yields instead).
pub fn start_session(
    deck: Deck,
    seed: u64,
    auto_pass: bool,
    record_replay: bool,
    replay_step: u64,
) -> (Session, usize) {
    let state = deck.build(seed);
    let initial_object_count = state.objects.len();
    let n = state.players.len() as u32;
    // Placeholder per-seat agents: a `Session` never consults them (its `ask` seam yields instead of
    // calling the blocking sink), but `Engine::new` installs them as the core's agent set.
    let agents: Vec<Box<dyn Agent>> = (0..n)
        .map(|s| Box::new(RandomAgent::new(seed ^ s as u64)) as Box<dyn Agent>)
        .collect();
    let mut engine = Engine::new(state, agents);
    if auto_pass {
        engine.set_arena_auto_pass(true);
    }
    if record_replay {
        engine.set_replay_source(ReplaySource::AiTraining { step: replay_step });
        engine.record_replay(true);
    }
    (Session::start(engine), initial_object_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_parse_accepts_aliases() {
        assert_eq!(Deck::parse("lands"), Some(Deck::LandsOnly));
        assert_eq!(Deck::parse("Demo"), Some(Deck::Demo));
        assert_eq!(Deck::parse("burn-vs-bears"), Some(Deck::BurnVsBears));
        assert_eq!(Deck::parse("heralds"), Some(Deck::Heralds));
        assert_eq!(Deck::parse("Bears"), Some(Deck::Bears));
        assert_eq!(Deck::parse("swine"), Some(Deck::Swine));
        assert_eq!(Deck::parse("nope"), None);
    }

    use mtg_core::session::Step;

    // A full game driven entirely through a `Session` by an in-thread "policy" that picks the first
    // legal factored slot every time — proves the resume/submit + summary assembly end to end
    // without Python. (The Python smoke test does the randomized, thousands-of-games version.)
    #[test]
    fn session_drives_a_game_to_completion() {
        let (mut sess, initial_object_count) = start_session(Deck::LandsOnly, 7, true, false, 0);
        let mut decisions = 0usize;
        let summary = loop {
            match sess.resume() {
                Step::Decision { view, request, .. } => {
                    // Drive the factored sub-steps to a commit by always taking the first legal slot.
                    let mut inter = crate::codec::Interaction::new(&view, &request);
                    let resp = loop {
                        decisions += 1;
                        let mask = inter.mask();
                        let slot = mask.iter().position(|b| *b).expect("non-empty mask");
                        if let Some(r) = inter.apply(slot) {
                            break r;
                        }
                    };
                    sess.submit(resp);
                }
                Step::GameOver { outcome } => {
                    // Session::replay() always reconstructs from the core once finished; with
                    // record_replay off there are simply no frames (PyGame gates on record_replay).
                    assert!(sess.replay().is_some_and(|r| r.frames.is_empty()), "no frames unless record_replay");
                    break end_summary_from(&outcome, sess.state().expect("finished state"), initial_object_count);
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

    // record_replay=true ⇒ Session::replay() yields a Replay with frames, the AiTraining source, and
    // the engine-filled result. created_at stays 0 (caller stamps it) — validates the schema.
    #[test]
    fn session_records_replay_when_enabled() {
        let (mut sess, _init) = start_session(Deck::LandsOnly, 3, true, true, 1234);
        let replay = loop {
            match sess.resume() {
                Step::Decision { view, request, .. } => {
                    let mut inter = crate::codec::Interaction::new(&view, &request);
                    let resp = loop {
                        let slot = inter.mask().iter().position(|b| *b).expect("non-empty mask");
                        if let Some(r) = inter.apply(slot) {
                            break r;
                        }
                    };
                    sess.submit(resp);
                }
                Step::GameOver { .. } => break sess.replay().expect("replay recorded"),
            }
        };
        assert!(replay.frames.len() > 1, "replay has frames");
        assert_eq!(replay.frames[0].label, "game start");
        assert_eq!(replay.meta.source, ReplaySource::AiTraining { step: 1234 });
        assert!(replay.meta.result.is_some(), "engine fills result at game end");
        assert_eq!(replay.meta.created_at, 0, "caller stamps the clock");
    }
}
