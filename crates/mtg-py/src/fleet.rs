//! M3.4 fleet stepper. A `Fleet` owns many games and advances the whole batch to its next factored
//! decisions in **one** PyO3 crossing, so the per-decision Python↔Rust round-trip that pegs one
//! Python core (the measured wall) collapses into a single call whose obs/masks cross as bytes
//! (`np.frombuffer`), never a per-element Python list.
//!
//! Each game is a [`GameSlot`] — a `Session` (M3 resumable engine) + its in-flight factored
//! [`Interaction`], i.e. exactly the per-game state `PyGame` holds, minus Python. `tick` steps every
//! non-terminal slot to its current sub-step and batch-encodes it; `submit` feeds one factored action
//! per env and advances. (Phase 1: single-threaded, every decision surfaced to Python. Phase 2 pins
//! groups of slots to worker threads for GIL-free parallel stepping + self-play opponent grouping.)

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use mtg_core::session::{Session, Step};

use crate::codec::{self, Interaction};
use crate::game::{self, end_summary_from, start_session, Deck};
use crate::obs;

/// One game's resumable engine + in-flight factored decision (PyGame's core, reusable off-Python).
struct GameSlot {
    session: Session,
    interaction: Option<Interaction>,
    initial_object_count: usize,
    seat: i64,
    terminal: bool,
    summary: Option<game::EndSummary>,
}

impl GameSlot {
    fn new(deck: Deck, seed: u64, auto_pass: bool) -> Self {
        let (session, initial_object_count) = start_session(deck, seed, auto_pass, false, 0);
        let mut slot = GameSlot {
            session,
            interaction: None,
            initial_object_count,
            seat: -1,
            terminal: false,
            summary: None,
        };
        slot.advance();
        slot
    }

    /// Advance to the next factored sub-step (or terminal). Mirrors `PyGame::advance` minus Python:
    /// continues an in-flight interaction, else `resume`s the session to the next engine decision.
    fn advance(&mut self) {
        if self.interaction.is_some() {
            return;
        }
        match self.session.resume() {
            Step::Decision { seat, view, request } => {
                self.seat = seat.0 as i64;
                self.interaction = Some(Interaction::new(&view, &request));
            }
            Step::GameOver { outcome } => {
                self.terminal = true;
                let state = self.session.state().expect("finished session exposes its state");
                self.summary = Some(end_summary_from(&outcome, state, self.initial_object_count));
                self.interaction = None;
            }
        }
    }

    /// Feed one factored action; on commit, submit the assembled response and advance to the next
    /// sub-step (so after `apply` the slot again sits at a decision or terminal, ready for the next tick).
    fn apply(&mut self, action: usize) {
        if let Some(inter) = self.interaction.as_mut() {
            if let Some(resp) = inter.apply(action) {
                self.session.submit(resp);
                self.interaction = None;
            }
        }
        self.advance();
    }

    fn mask(&self) -> Vec<bool> {
        self.interaction
            .as_ref()
            .map(|i| i.mask())
            .unwrap_or_else(|| vec![false; codec::ACTION_DIM])
    }
    fn num_legal(&self) -> usize {
        self.interaction.as_ref().map(|i| i.num_legal()).unwrap_or(0)
    }
    fn request_name(&self) -> &'static str {
        self.interaction
            .as_ref()
            .map(|i| crate::request_name(i.req()))
            .unwrap_or("Terminal")
    }
    fn obs(&self) -> Option<obs::Obs> {
        self.interaction
            .as_ref()
            .map(|i| obs::encode(i.view(), i.req(), i.num_legal()))
    }
}

/// A batch of games ticked in one PyO3 crossing. Phase-1: single-threaded; every decision (both
/// seats) is surfaced to Python, which answers with `submit`. `unsendable`: the slots' Sessions are
/// fibers pinned to this thread.
#[pyclass(unsendable)]
pub struct Fleet {
    deck: Deck,
    slots: Vec<GameSlot>,
}

#[pymethods]
impl Fleet {
    /// Build `num_envs` games of `deck`, seeded `base_seed + i`, each advanced to its first decision.
    #[new]
    #[pyo3(signature = (deck, num_envs, auto_pass = true, base_seed = 0))]
    fn new(deck: &str, num_envs: usize, auto_pass: bool, base_seed: u64) -> PyResult<Self> {
        let deck = Deck::parse(deck)
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("unknown deck {deck:?}")))?;
        let slots = (0..num_envs)
            .map(|i| GameSlot::new(deck, base_seed.wrapping_add(i as u64), auto_pass))
            .collect();
        Ok(Fleet { deck, slots })
    }

    fn num_envs(&self) -> usize {
        self.slots.len()
    }
    fn action_dim(&self) -> usize {
        codec::ACTION_DIM
    }
    fn card_vocab(&self) -> Vec<u32> {
        self.deck.vocab()
    }

    // ── per-env accessors (used by the equivalence _FleetDriver on a 1-env fleet) ────────────────
    fn seat(&self, i: usize) -> i64 {
        self.slots[i].seat
    }
    fn request(&self, i: usize) -> String {
        self.slots[i].request_name().to_string()
    }
    fn num_legal(&self, i: usize) -> usize {
        self.slots[i].num_legal()
    }
    fn env_mask(&self, i: usize) -> Vec<bool> {
        self.slots[i].mask()
    }
    fn terminal(&self, i: usize) -> bool {
        self.slots[i].terminal
    }
    /// `(winner, turns, reason)` once env `i` is terminal, else `None`.
    fn summary(&self, i: usize) -> Option<(Option<i64>, u32, String)> {
        self.slots[i]
            .summary
            .map(|s| (s.winner.map(|w| w as i64), s.turns, s.reason.to_string()))
    }

    /// Apply one factored action per env (indexed 0..num_envs) and advance each to its next decision
    /// (or terminal). Terminal envs ignore their action. Returns nothing — read the batch via `tick`.
    fn submit(&mut self, actions: Vec<usize>) -> PyResult<()> {
        if actions.len() != self.slots.len() {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "submit expects {} actions, got {}",
                self.slots.len(),
                actions.len()
            )));
        }
        for (slot, &a) in self.slots.iter_mut().zip(actions.iter()) {
            if !slot.terminal {
                slot.apply(a);
            }
        }
        Ok(())
    }

    /// Batch-encode the current sub-step of every env into flat buffers, crossing to Python once as
    /// bytes: `{obs arrays → bytes, "mask" → bytes(u8), "seat"/"num_legal"/"terminal" → bytes(i32)}`.
    /// Python does `np.frombuffer(...).reshape(num_envs, ...)` (shapes from `PyGame.obs_spec()` +
    /// `action_dim`). Terminal envs contribute a zero row + a terminal flag.
    fn tick<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let n = self.slots.len();
        let ad = codec::ACTION_DIM;
        // Preallocate per-array flat buffers (num_envs rows). Shapes mirror obs::spec().
        let g = obs::G;
        let mut globals = vec![0f32; n * g];
        let mut mask = vec![0u8; n * ad];
        let mut seat = vec![0i32; n];
        let mut num_legal = vec![0i32; n];
        let mut terminal = vec![0i32; n];
        // Variable obs arrays flattened by their spec dims.
        let spec = obs::spec();
        let mut arrays: Vec<(&'static str, usize, Vec<f32>, Vec<i64>, bool)> = spec
            .iter()
            .filter(|(name, ..)| *name != "globals")
            .map(|&(name, rows, cols, is_int)| (name, rows * cols, vec![0f32; n * rows * cols], vec![0i64; n * rows * cols], is_int))
            .collect();

        for (i, slot) in self.slots.iter().enumerate() {
            seat[i] = slot.seat as i32;
            num_legal[i] = slot.num_legal() as i32;
            terminal[i] = slot.terminal as i32;
            for (j, b) in slot.mask().iter().enumerate() {
                mask[i * ad + j] = *b as u8;
            }
            if let Some(o) = slot.obs() {
                globals[i * g..(i + 1) * g].copy_from_slice(&o.globals);
                for (name, width, fbuf, ibuf, is_int) in arrays.iter_mut() {
                    let (src_f, src_i): (&[f32], &[i64]) = match *name {
                        "bf_feat" => (&o.bf_feat, &[]),
                        "bf_ids" => (&[], &o.bf_ids),
                        "hand_feat" => (&o.hand_feat, &[]),
                        "hand_ids" => (&[], &o.hand_ids),
                        "stack_feat" => (&o.stack_feat, &[]),
                        "stack_ids" => (&[], &o.stack_ids),
                        "decision_ids" => (&[], &o.decision_ids),
                        _ => (&[], &[]),
                    };
                    if *is_int {
                        ibuf[i * *width..(i + 1) * *width].copy_from_slice(src_i);
                    } else {
                        fbuf[i * *width..(i + 1) * *width].copy_from_slice(src_f);
                    }
                }
            }
        }

        let out = PyDict::new(py);
        out.set_item("globals", PyBytes::new(py, bytemuck_f32(&globals)))?;
        for (name, _w, fbuf, ibuf, is_int) in &arrays {
            let bytes = if *is_int { bytemuck_i64(ibuf) } else { bytemuck_f32(fbuf) };
            out.set_item(*name, PyBytes::new(py, bytes))?;
        }
        out.set_item("mask", PyBytes::new(py, &mask))?;
        out.set_item("seat", PyBytes::new(py, bytemuck_i32(&seat)))?;
        out.set_item("num_legal", PyBytes::new(py, bytemuck_i32(&num_legal)))?;
        out.set_item("terminal", PyBytes::new(py, bytemuck_i32(&terminal)))?;
        Ok(out)
    }
}

// Reinterpret POD slices as bytes for the one-crossing transfer (little-endian, matched by the
// Python side's np.frombuffer dtype). No external crate: these are plain `#[repr]` scalars.
fn bytemuck_f32(s: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}
fn bytemuck_i64(s: &[i64]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}
fn bytemuck_i32(s: &[i32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}
