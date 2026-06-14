//! `mtg_py` — the PyO3 extension module: a thin Python handle ([`PyGame`]) over the `mtg-core`
//! `Agent` boundary (GYM_PLAN L3). **No rules logic lives here** (repo law) — the engine runs on
//! its own thread ([`game`]) and this crate only ferries decisions across the FFI seam, projecting
//! each through the swappable observation encoder ([`obs`]) and action codec ([`codec`]).
//!
//! Python surface (GYM_PLAN §2.3), all on `PyGame`:
//! - `reset(seed) -> StepTuple` — start a fresh game, advance to the first decision (sub-step)
//! - `step_to_decision() -> StepTuple` — advance to the next decision sub-step (call AFTER `apply`)
//! - `apply(action)` — feed one factored action; the engine request is answered only when the
//!   autoregressive sub-steps commit (multi-target / combat / multi-select decompose into several
//!   `apply`s; GYM_PLAN §4.2)
//! - `legal_mask() -> list[bool]` — constant-width mask for the current sub-step
//! - `obs_spec()` — the structured-observation layout (Python builds its `gym.spaces.Dict` from it)
//! - `outcome() -> int | None`, `summary()`, `is_terminal()` — terminal readouts
//! - `snapshot`/`restore`/`clone` — stubbed (need the milestone-3 resumable step API)
//!
//! A `StepTuple` is `(obs, mask, seat, request, num_legal, terminal)` where `obs` is a dict of
//! lists (the [`obs::Obs`] arrays). The Python `MtgEnv` turns it into Gym `obs`/`info`.

mod codec;
mod game;
mod layout;
mod obs;

use pyo3::exceptions::{PyNotImplementedError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use mtg_core::agent::DecisionRequest;
use mtg_core::replay::Replay;

use codec::Interaction;
use game::{Deck, FromGame, GameConn};

/// `(obs_dict, mask, seat, request_name, num_legal, terminal)`.
type StepTuple = (PyObject, Vec<bool>, i64, String, usize, bool);

/// A single in-process game, driven pull-style from Python. Owns the game thread (`conn`), the
/// request receiver (`from_game`, owned here so it can be moved into the GIL-released blocking
/// recv — `std`'s channels are `Send` but `!Sync`), and the in-flight decision's autoregressive
/// [`Interaction`].
///
/// `unsendable`: this handle owns OS threads + channel ends, so it is pinned to its creating thread
/// (PyO3 raises if touched from another) — the normal one-env-per-thread / per-subprocess Gym use.
#[pyclass(unsendable)]
pub struct PyGame {
    deck: Deck,
    auto_pass: bool,
    record_replay: bool,
    replay_step: u64,
    conn: Option<GameConn>,
    from_game: Option<std::sync::mpsc::Receiver<FromGame>>,
    /// The current engine decision, decomposed into factored sub-steps. `Some` while a decision is
    /// in flight (possibly mid-autoregression); cleared when its response is committed.
    interaction: Option<Interaction>,
    seat: i64,
    terminal: bool,
    summary: Option<game::EndSummary>,
    /// The omniscient replay of the finished game (`Some` iff constructed with `record_replay`),
    /// awaiting `created_at`/names stamping in [`PyGame::replay_json`].
    replay: Option<Replay>,
}

#[pymethods]
impl PyGame {
    /// `deck` ∈ {`"lands"`, `"demo"`, `"burn_vs_bears"`}. `auto_pass` enables the Arena-profile
    /// auto-pass (fewer trivial priority windows; still deterministic per seed+profile).
    /// `record_replay` records an omniscient replay tagged `AiTraining{replay_step}`, retrievable
    /// after the game via [`replay_json`](PyGame::replay_json) (training-replay export).
    #[new]
    #[pyo3(signature = (deck = "demo", auto_pass = true, record_replay = false, replay_step = 0))]
    fn new(deck: &str, auto_pass: bool, record_replay: bool, replay_step: u64) -> PyResult<Self> {
        let deck = Deck::parse(deck)
            .ok_or_else(|| PyRuntimeError::new_err(format!("unknown deck {deck:?}")))?;
        Ok(PyGame {
            deck,
            auto_pass,
            record_replay,
            replay_step,
            conn: None,
            from_game: None,
            interaction: None,
            seat: -1,
            terminal: false,
            summary: None,
            replay: None,
        })
    }

    /// The sorted unique `grp_id`s across both decks — the card-identity vocabulary. The Python obs
    /// layer one-hots each card row against this (deck-determined card identity, GYM_PLAN §3).
    fn card_vocab(&self) -> Vec<u32> {
        self.deck.vocab()
    }

    /// Tear down any running game, start a fresh one for `seed`, and advance to the first decision
    /// sub-step.
    fn reset(&mut self, py: Python<'_>, seed: u64) -> PyResult<StepTuple> {
        self.conn = None;
        self.from_game = None;
        self.interaction = None;
        self.seat = -1;
        self.terminal = false;
        self.summary = None;
        self.replay = None;

        let (conn, from_game) =
            GameConn::spawn(self.deck, seed, self.auto_pass, self.record_replay, self.replay_step);
        self.conn = Some(conn);
        self.from_game = Some(from_game);
        self.advance(py)
    }

    /// Advance to the next decision sub-step (or terminal). Call AFTER [`apply`](PyGame::apply).
    fn step_to_decision(&mut self, py: Python<'_>) -> PyResult<StepTuple> {
        self.advance(py)
    }

    /// Feed one factored action for the current sub-step. If it completes the engine decision
    /// (commit), the assembled `DecisionResponse` is sent to the game thread; otherwise the
    /// interaction keeps accumulating and the next `step_to_decision` returns the next sub-step.
    fn apply(&mut self, action: usize) -> PyResult<()> {
        let inter = self.interaction.as_mut().ok_or_else(|| {
            PyRuntimeError::new_err(
                "apply() with no pending decision (reset/step_to_decision first, or game is over)",
            )
        })?;
        if let Some(resp) = inter.apply(action) {
            // Decision complete — answer the engine and clear so the next advance pulls anew.
            match &self.conn {
                Some(conn) => {
                    conn.respond(resp);
                }
                None => return Err(PyRuntimeError::new_err("apply() before reset()")),
            }
            self.interaction = None;
        }
        Ok(())
    }

    /// The constant-width legality mask for the current sub-step (all-false when terminal).
    fn legal_mask(&self) -> Vec<bool> {
        match &self.interaction {
            Some(i) => i.mask(),
            None => vec![false; codec::ACTION_DIM],
        }
    }

    /// The structured-observation layout: a list of `(name, rows, cols, is_int)`. Python builds its
    /// `gym.spaces.Dict` from this, so shapes are never hard-coded on the Python side.
    #[staticmethod]
    fn obs_spec() -> Vec<(String, usize, usize, bool)> {
        obs::spec()
            .into_iter()
            .map(|(n, r, c, i)| (n.to_string(), r, c, i))
            .collect()
    }

    #[staticmethod]
    fn action_dim() -> usize {
        codec::ACTION_DIM
    }

    /// Winning seat index, or `None` (draw / not finished).
    fn outcome(&self) -> Option<i64> {
        self.summary.and_then(|s| s.winner.map(|w| w as i64))
    }

    /// `(winner, turns, reason, initial_object_count, object_count, zone_sum)` once terminal.
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

    /// Serialize the recorded omniscient replay to a JSON string (training-replay export,
    /// REPLAY_PLAN §3). Stamps `created_at` (unix ms — the caller supplies the clock, the core has
    /// none) and, optionally, the per-seat player `names`/`decks`. `meta.source` is already
    /// `AiTraining{replay_step}` and `meta.result` is filled by the engine. Returns `None` if the
    /// game wasn't built with `record_replay=True` (or hasn't finished). Python writes the string
    /// to `data/replays/<id>.json`.
    #[pyo3(signature = (created_at, names = None, decks = None))]
    fn replay_json(
        &self,
        created_at: i64,
        names: Option<Vec<String>>,
        decks: Option<Vec<String>>,
    ) -> PyResult<Option<String>> {
        let Some(replay) = &self.replay else {
            return Ok(None);
        };
        let mut replay = replay.clone();
        replay.meta.created_at = created_at;
        if let Some(names) = names {
            for (p, name) in replay.meta.players.iter_mut().zip(names) {
                p.name = name;
            }
        }
        if let Some(decks) = decks {
            for (p, deck) in replay.meta.players.iter_mut().zip(decks) {
                p.deck = deck;
            }
        }
        serde_json::to_string(&replay)
            .map(Some)
            .map_err(|e| PyRuntimeError::new_err(format!("replay serialize: {e}")))
    }

    // ── milestone-3 stubs (need the resumable step API; not in approach-A) ──────────────────
    fn snapshot(&self) -> PyResult<Vec<u8>> {
        Err(PyNotImplementedError::new_err(
            "snapshot/restore/clone require the resumable step API (GYM_PLAN milestone 3)",
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
    /// Either continue the in-flight decision's next sub-step (no game-thread round-trip) or, when
    /// the previous decision committed, block (GIL released) for the next engine request.
    fn advance(&mut self, py: Python<'_>) -> PyResult<StepTuple> {
        // Continuation: an interaction is in flight and not yet committed → next sub-step.
        if self.interaction.is_some() {
            return Ok(self.decision_tuple(py));
        }

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
                self.seat = seat.0 as i64;
                self.interaction = Some(Interaction::new(&view, &req));
                Ok(self.decision_tuple(py))
            }
            Ok(FromGame::GameOver { summary, replay }) => {
                self.terminal = true;
                self.summary = Some(summary);
                self.replay = replay;
                self.interaction = None;
                Ok(self.terminal_tuple(py, "GameOver"))
            }
            Err(_) => {
                self.terminal = true;
                self.interaction = None;
                Ok(self.terminal_tuple(py, "Closed"))
            }
        }
    }

    /// Build the `StepTuple` for the current interaction's sub-step.
    fn decision_tuple(&self, py: Python<'_>) -> StepTuple {
        let inter = self.interaction.as_ref().expect("interaction present");
        let num_legal = inter.num_legal();
        let mask = inter.mask();
        let o = obs::encode(inter.view(), inter.req(), num_legal);
        let obs_dict = obs_to_py(py, &o);
        let name = request_name(inter.req()).to_string();
        (obs_dict, mask, self.seat, name, num_legal, false)
    }

    fn terminal_tuple(&self, py: Python<'_>, name: &str) -> StepTuple {
        (
            zeros_obs(py),
            vec![false; codec::ACTION_DIM],
            -1,
            name.to_string(),
            0,
            true,
        )
    }
}

/// Convert the structured observation into a Python dict of lists (Python reshapes per `obs_spec`).
fn obs_to_py(py: Python<'_>, o: &obs::Obs) -> PyObject {
    let d = PyDict::new(py);
    d.set_item("globals", &o.globals).unwrap();
    d.set_item("bf_feat", &o.bf_feat).unwrap();
    d.set_item("bf_ids", &o.bf_ids).unwrap();
    d.set_item("hand_feat", &o.hand_feat).unwrap();
    d.set_item("hand_ids", &o.hand_ids).unwrap();
    d.set_item("stack_feat", &o.stack_feat).unwrap();
    d.set_item("stack_ids", &o.stack_ids).unwrap();
    d.into_any().unbind()
}

/// A zero observation (correct shapes) for terminal steps.
fn zeros_obs(py: Python<'_>) -> PyObject {
    let d = PyDict::new(py);
    for (name, rows, cols, is_int) in obs::spec() {
        if is_int {
            d.set_item(name, vec![0i64; rows * cols]).unwrap();
        } else {
            d.set_item(name, vec![0f32; rows * cols]).unwrap();
        }
    }
    d.into_any().unbind()
}

/// Short stable name of a request variant (for the Python `info` / debugging).
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
    m.add("ACTION_DIM", codec::ACTION_DIM)?;
    Ok(())
}
