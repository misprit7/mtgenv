//! `mtg_py` — the PyO3 extension module: a thin Python handle ([`PyGame`]) over the `mtg-core`
//! `Agent` boundary (GYM_PLAN L3). **No rules logic lives here** (repo law) — the engine runs on
//! its own thread ([`game`]) and this crate only ferries decisions across the FFI seam, projecting
//! each through the swappable observation encoder ([`obs`]) and action codec ([`codec`]).
//!
//! Python surface (GYM_PLAN §2.3), all on `PyGame`:
//! - `reset(seed) -> StepTuple` — start a fresh game, advance to the first decision
//! - `step_to_decision() -> StepTuple` — advance to the next decision (call AFTER `apply`)
//! - `apply(action)` — submit the decoded `DecisionResponse` for the current decision
//! - `legal_mask() -> list[bool]` — constant-width mask for the current decision
//! - `outcome() -> int | None`, `summary()`, `is_terminal()` — terminal readouts
//! - `snapshot`/`restore`/`clone` — stubbed (need the milestone-3 resumable step API)
//!
//! A `StepTuple` is `(obs, mask, seat, request, num_legal, terminal)`:
//! `(list[float], list[bool], int, str, int, bool)`. The Python `MtgEnv` (python/mtgenv_gym)
//! turns it into Gym `obs`/`info`.

mod codec;
mod game;
mod obs;

use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError};
use pyo3::prelude::*;

use mtg_core::agent::DecisionRequest;

use game::{Deck, FromGame, GameConn};

/// `(obs, mask, seat, request_name, num_legal, terminal)`.
type StepTuple = (Vec<f32>, Vec<bool>, i64, String, usize, bool);

/// What the engine is currently asking this game — cached so `apply`/`legal_mask` answer the
/// *same* enumeration the observation was built from.
struct Pending {
    options: Vec<mtg_core::agent::DecisionResponse>,
}

/// A single in-process game, driven pull-style from Python. Owns the game thread (via `conn`) and
/// the request receiver (`from_game`, owned here so it can be moved into the GIL-released blocking
/// recv — `std`'s `Receiver`/`Sender` are `Send` but `!Sync`).
///
/// `unsendable`: this handle owns OS threads + channel ends (`!Sync`), so it is pinned to the
/// thread that created it — PyO3 raises if it's touched from another thread. That matches how a
/// Gym env is used (one env per thread / per subprocess); it never crosses threads silently.
#[pyclass(unsendable)]
pub struct PyGame {
    deck: Deck,
    auto_pass: bool,
    conn: Option<GameConn>,
    from_game: Option<std::sync::mpsc::Receiver<FromGame>>,
    pending: Option<Pending>,
    terminal: bool,
    summary: Option<game::EndSummary>,
}

#[pymethods]
impl PyGame {
    /// `deck` ∈ {`"lands"`, `"demo"`, `"burn_vs_bears"`}. `auto_pass` enables the Arena-profile
    /// auto-pass (fewer trivial priority windows; still deterministic per seed+profile).
    #[new]
    #[pyo3(signature = (deck = "demo", auto_pass = true))]
    fn new(deck: &str, auto_pass: bool) -> PyResult<Self> {
        let deck = Deck::parse(deck)
            .ok_or_else(|| PyRuntimeError::new_err(format!("unknown deck {deck:?}")))?;
        Ok(PyGame {
            deck,
            auto_pass,
            conn: None,
            from_game: None,
            pending: None,
            terminal: false,
            summary: None,
        })
    }

    /// Tear down any running game, start a fresh one for `seed`, and advance to the first
    /// decision. Returns the first `StepTuple`.
    fn reset(&mut self, py: Python<'_>, seed: u64) -> PyResult<StepTuple> {
        // Dropping the old `GameConn` joins its thread (the fallback agent finishes it); replacing
        // `from_game` discards any buffered messages from the old game.
        self.conn = None;
        self.from_game = None;
        self.pending = None;
        self.terminal = false;
        self.summary = None;

        let (conn, from_game) = GameConn::spawn(self.deck, seed, self.auto_pass);
        self.conn = Some(conn);
        self.from_game = Some(from_game);
        self.advance(py)
    }

    /// Advance the game to its next decision (or terminal). Call this AFTER [`apply`](PyGame::apply)
    /// — calling it with an un-applied decision pending would block forever (the game thread is
    /// waiting on the response).
    fn step_to_decision(&mut self, py: Python<'_>) -> PyResult<StepTuple> {
        self.advance(py)
    }

    /// Submit the action for the current decision: decode it through the codec into a
    /// `DecisionResponse` and hand it to the waiting game thread.
    fn apply(&mut self, action: usize) -> PyResult<()> {
        let pending = self.pending.take().ok_or_else(|| {
            PyRuntimeError::new_err(
                "apply() with no pending decision (reset/step_to_decision first, or game is over)",
            )
        })?;
        let resp = codec::decode(&pending.options, action);
        match &self.conn {
            Some(conn) => {
                conn.respond(resp);
                Ok(())
            }
            None => Err(PyRuntimeError::new_err("apply() before reset()")),
        }
    }

    /// The constant-width legality mask for the current decision (all-false when terminal / no
    /// decision pending).
    fn legal_mask(&self) -> Vec<bool> {
        match &self.pending {
            Some(p) => codec::mask_from_options(&p.options),
            None => vec![false; codec::ACTION_DIM],
        }
    }

    /// Winning seat index, or `None` (draw / not finished).
    fn outcome(&self) -> Option<i64> {
        self.summary.and_then(|s| s.winner.map(|w| w as i64))
    }

    /// `(winner, turns, reason, initial_object_count, object_count, zone_sum)` once terminal —
    /// the conservation invariants for the smoke test. `None` before the game ends.
    fn summary(&self) -> Option<(Option<i64>, u32, String, usize, usize, usize)> {
        self.summary.map(|s| {
            (
                s.winner.map(|w| w as i64),
                s.turns,
                s.reason.to_string(),
                s.initial_object_count,
                s.object_count,
                s.zone_sum,
            )
        })
    }

    fn is_terminal(&self) -> bool {
        self.terminal
    }

    #[staticmethod]
    fn obs_dim() -> usize {
        obs::OBS_DIM
    }

    #[staticmethod]
    fn action_dim() -> usize {
        codec::ACTION_DIM
    }

    // ── milestone-3 stubs (need the resumable step API; not in approach-A) ──────────────────
    fn snapshot(&self) -> PyResult<Vec<u8>> {
        Err(PyNotImplementedError::new_err(
            "snapshot/restore/clone require the resumable step API (GYM_PLAN milestone 3); \
             the thread+channel bridge keeps state on the game thread",
        ))
    }
    fn restore(&mut self, _data: Vec<u8>) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(
            "restore requires the resumable step API (GYM_PLAN milestone 3)",
        ))
    }
    fn clone_game(&self) -> PyResult<PyGame> {
        Err(PyNotImplementedError::new_err(
            "clone requires the resumable step API (GYM_PLAN milestone 3)",
        ))
    }
}

impl PyGame {
    /// Block (GIL released) until the game thread yields the next decision or finishes, then build
    /// the `StepTuple`. The `Receiver` is moved into the closure and back out, because `std`'s
    /// `Receiver` is `Send` but `!Sync` — it can't be *borrowed* across the `allow_threads` seam.
    fn advance(&mut self, py: Python<'_>) -> PyResult<StepTuple> {
        let rx = self
            .from_game
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("no game running (call reset() first)"))?;
        let (rx, msg) = py.allow_threads(move || {
            let m = rx.recv();
            (rx, m)
        });
        self.from_game = Some(rx);

        match msg {
            Ok(FromGame::Decision { seat, view, req }) => {
                let options = codec::legal_options(&req);
                let mask = codec::mask_from_options(&options);
                let num_legal = options.len();
                let obs = obs::encode(&view, &req, num_legal);
                let name = request_name(&req).to_string();
                self.pending = Some(Pending { options });
                Ok((obs, mask, seat.0 as i64, name, num_legal, false))
            }
            Ok(FromGame::GameOver(summary)) => {
                self.terminal = true;
                self.summary = Some(summary);
                self.pending = None;
                Ok(terminal_tuple("GameOver"))
            }
            Err(_) => {
                // Game thread vanished without a GameOver (shouldn't happen in practice).
                self.terminal = true;
                self.pending = None;
                Ok(terminal_tuple("Closed"))
            }
        }
    }
}

fn terminal_tuple(name: &str) -> StepTuple {
    (
        vec![0.0; obs::OBS_DIM],
        vec![false; codec::ACTION_DIM],
        -1,
        name.to_string(),
        0,
        true,
    )
}

/// Short stable name of a request variant (for the Python `info` / debugging). Mirrors
/// [`obs::request_index`]'s ordering.
fn request_name(req: &DecisionRequest) -> &'static str {
    use DecisionRequest as Q;
    match req {
        Q::ChooseStartingPlayer { .. } => "ChooseStartingPlayer",
        Q::Mulligan { .. } => "Mulligan",
        Q::Priority { .. } => "Priority",
        Q::ChooseModes { .. } => "ChooseModes",
        Q::ChooseNumber { .. } => "ChooseNumber",
        Q::CastingTimeOptions { .. } => "CastingTimeOptions",
        Q::ChooseTargets { .. } => "ChooseTargets",
        Q::Distribute { .. } => "Distribute",
        Q::PayCost { .. } => "PayCost",
        Q::DeclareAttackers { .. } => "DeclareAttackers",
        Q::DeclareBlockers { .. } => "DeclareBlockers",
        Q::AssignCombatDamage { .. } => "AssignCombatDamage",
        Q::OrderObjects { .. } => "OrderObjects",
        Q::SelectCards { .. } => "SelectCards",
        Q::SelectFromGroups { .. } => "SelectFromGroups",
        Q::ArrangeCards { .. } => "ArrangeCards",
        Q::ChooseReplacement { .. } => "ChooseReplacement",
        Q::ChooseCounterType { .. } => "ChooseCounterType",
        Q::ChooseOption { .. } => "ChooseOption",
        Q::ChooseColor { .. } => "ChooseColor",
        Q::Confirm { .. } => "Confirm",
    }
}

/// The Python extension module `mtg_py`.
#[pymodule]
fn mtg_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyGame>()?;
    m.add("OBS_DIM", obs::OBS_DIM)?;
    m.add("ACTION_DIM", codec::ACTION_DIM)?;
    Ok(())
}
