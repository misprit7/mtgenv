//! M3.4 fleet stepper. A `Fleet` advances many games to their next factored decisions and hands
//! Python the whole batch in **one** PyO3 crossing, so the per-decision Python↔Rust round-trip that
//! pegs one Python core (the measured wall) collapses into a single call whose obs/masks cross as
//! bytes (`np.frombuffer`), never a per-element Python list.
//!
//! **Parallelism.** Each game is a [`GameSlot`] (a `Session` + its in-flight factored [`Interaction`]
//! — exactly PyGame's core, off-Python). A `Session` is a fiber pinned to its thread, so slots are
//! partitioned into fixed groups, one per **worker thread** that both *creates* and *steps* its
//! group. A `step` releases the GIL, fans the per-group actions out to the workers, and each worker
//! applies + advances + encodes its group into a plain-data [`GroupBatch`] sent back over a channel;
//! the main thread stitches the batches into contiguous buffers. No `Send` on the engine, no shared
//! mutable buffer — batches are owned data moved over channels. Python still runs every forward
//! (learner + per-checkpoint opponent groups in Phase 2); the fleet only parallelizes the stepping.
//!
//! TODO(gym-owner): `GameSlot` duplicates `PyGame`'s advance/apply/encode logic (lib.rs). Once the
//! fleet path is the primary one, hoist `GameSlot` to the canonical per-game core and make `PyGame`
//! a thin wrapper over it (one code path).

use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Instant;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use mtg_core::session::{Session, Step};

use crate::codec::{self, Interaction};
use crate::decision_stats;
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
    /// Semantic record of the decision the last `apply` finalized (tracked-stats, #68) — empty between
    /// finalizations. Mirrors `PyGame::last_stats`; captured per env so the fleet path feeds TrackedStats.
    last_stats: Vec<(&'static str, f64)>,
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
            last_stats: Vec::new(),
        };
        slot.advance();
        slot
    }

    /// Advance to the next factored sub-step (or terminal): continue an in-flight interaction, else
    /// `resume` the session to the next engine decision. Mirrors `PyGame::advance` minus Python.
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

    /// Feed one factored action; on commit, submit the assembled response, then advance to the next
    /// sub-step (so the slot again sits at a decision or terminal, ready for the next tick).
    fn apply(&mut self, action: usize) {
        let mut committed: Option<Vec<(&'static str, f64)>> = None;
        if let Some(inter) = self.interaction.as_mut() {
            if let Some(resp) = inter.apply(action) {
                // On commit, record the decision's semantic stats (req + resp) before submitting —
                // mirrors PyGame::step_to_decision so TrackedStats works on the fleet path too.
                committed = Some(decision_stats::summarize(inter.req(), &resp));
                self.session.submit(resp);
                self.interaction = None;
            }
        }
        self.last_stats = committed.unwrap_or_default();
        self.advance();
    }

    fn request_name(&self) -> &'static str {
        self.interaction
            .as_ref()
            .map(|i| crate::request_name(i.req()))
            .unwrap_or("Terminal")
    }
}

/// One worker's encoded slice of the batch (contiguous env rows in group order). Plain data — `Send`
/// — so it moves back to the main thread over a channel with no engine types crossing threads.
struct GroupBatch {
    globals: Vec<f32>,
    bf_feat: Vec<f32>,
    bf_grpid: Vec<i64>,
    hand_feat: Vec<f32>,
    hand_grpid: Vec<i64>,
    stack_feat: Vec<f32>,
    stack_grpid: Vec<i64>,
    decision_grpid: Vec<i64>,
    edges: Vec<i64>,
    choice_feat: Vec<f32>,
    mask: Vec<u8>,
    seat: Vec<i32>,
    num_legal: Vec<i32>,
    terminal: Vec<i32>,
    requests: Vec<&'static str>,
    summaries: Vec<Option<(Option<i64>, u32, String)>>,
    stats: Vec<Vec<(&'static str, f64)>>, // per-env decision_stats (empty when the sub-step didn't finalize)
}

impl GroupBatch {
    fn with_capacity(k: usize) -> Self {
        GroupBatch {
            globals: Vec::with_capacity(k * obs::G),
            bf_feat: Vec::new(),
            bf_grpid: Vec::new(),
            hand_feat: Vec::new(),
            hand_grpid: Vec::new(),
            stack_feat: Vec::new(),
            stack_grpid: Vec::new(),
            decision_grpid: Vec::new(),
            edges: Vec::new(),
            choice_feat: Vec::new(),
            mask: Vec::with_capacity(k * codec::ACTION_DIM),
            seat: Vec::with_capacity(k),
            num_legal: Vec::with_capacity(k),
            terminal: Vec::with_capacity(k),
            requests: Vec::with_capacity(k),
            summaries: Vec::with_capacity(k),
            stats: Vec::with_capacity(k),
        }
    }
}

/// `rows*cols` of a named obs array (its flat row width), from `obs::spec()`.
fn arr_width(name: &str) -> usize {
    obs::spec().iter().find(|(n, ..)| *n == name).map(|(_, r, c, _)| r * c).expect("obs array in spec")
}

/// Encode one group of slots into a `GroupBatch` (contiguous rows). Terminal envs get a zero obs row
/// (still a full-width row) + the terminal flag; their summary rides along once, when they end.
fn encode_group(slots: &[GameSlot]) -> GroupBatch {
    let ad = codec::ACTION_DIM;
    let mut b = GroupBatch::with_capacity(slots.len());
    for slot in slots {
        b.seat.push(slot.seat as i32);
        b.num_legal.push(slot.interaction.as_ref().map(|i| i.num_legal()).unwrap_or(0) as i32);
        b.terminal.push(slot.terminal as i32);
        b.requests.push(slot.request_name());
        b.summaries
            .push(slot.summary.map(|s| (s.winner.map(|w| w as i64), s.turns, s.reason.to_string())));
        b.stats.push(slot.last_stats.clone()); // per-env decision_stats (empty for a non-final sub-step)
        // mask (full width, all-false when terminal)
        match &slot.interaction {
            Some(i) => b.mask.extend(i.mask().iter().map(|&x| x as u8)),
            None => b.mask.extend(std::iter::repeat(0u8).take(ad)),
        }
        // obs arrays (encode, or zero rows when terminal)
        let o = slot.interaction.as_ref().map(|i| {
            let (blocks, block_source) = i.pending_block_view();
            let pending = obs::PendingView {
                blocks,
                block_source,
                attackers: i.pending_attackers(),
                target_picks: i.pending_target_picks(),
                choices: i.choice_rows(),
            };
            obs::encode(i.view(), i.req(), i.num_legal(), &pending)
        });
        match o {
            Some(o) => {
                b.globals.extend_from_slice(&o.globals);
                b.bf_feat.extend_from_slice(&o.bf_feat);
                b.bf_grpid.extend_from_slice(&o.bf_grpid);
                b.hand_feat.extend_from_slice(&o.hand_feat);
                b.hand_grpid.extend_from_slice(&o.hand_grpid);
                b.stack_feat.extend_from_slice(&o.stack_feat);
                b.stack_grpid.extend_from_slice(&o.stack_grpid);
                b.decision_grpid.extend_from_slice(&o.decision_grpid);
                b.edges.extend_from_slice(&o.edges);
                b.choice_feat.extend_from_slice(&o.choice_feat);
            }
            None => {
                // A terminal env contributes a full-width zero row for every array
                // (edges pad with −1 — an all-zero edge row would read as a real edge).
                b.globals.extend(std::iter::repeat(0f32).take(obs::G));
                b.bf_feat.extend(std::iter::repeat(0f32).take(arr_width("bf_feat")));
                b.bf_grpid.extend(std::iter::repeat(0i64).take(arr_width("bf_grpid")));
                b.hand_feat.extend(std::iter::repeat(0f32).take(arr_width("hand_feat")));
                b.hand_grpid.extend(std::iter::repeat(0i64).take(arr_width("hand_grpid")));
                b.stack_feat.extend(std::iter::repeat(0f32).take(arr_width("stack_feat")));
                b.stack_grpid.extend(std::iter::repeat(0i64).take(arr_width("stack_grpid")));
                b.decision_grpid.extend(std::iter::repeat(0i64).take(arr_width("decision_grpid")));
                b.edges.extend(std::iter::repeat(-1i64).take(arr_width("edges")));
                b.choice_feat.extend(std::iter::repeat(0f32).take(arr_width("choice_feat")));
            }
        }
    }
    b
}

/// One coordinated advance for a worker's group: apply `steps` (local slot index → factored action,
/// advancing only those slots — so the self-play pump can advance opponent-pending envs while learner
/// envs wait), then `resets` (local slot index → fresh-game seed, for auto-reset on terminal). The
/// worker re-encodes its whole group afterward so the assembled batch stays consistent.
struct StepMsg {
    steps: Vec<(usize, usize)>,
    resets: Vec<(usize, u64)>,
}

enum Command {
    Step(StepMsg),
    Shutdown,
}

struct Worker {
    tx: Sender<Command>,
    rx: Receiver<GroupBatch>,
    handle: Option<JoinHandle<()>>,
    range: (usize, usize), // [start, end) env indices this worker owns
}

/// Batch of games ticked in one PyO3 crossing, stepped by `num_workers` pinned threads. `unsendable`:
/// this handle owns the worker threads; the engine fibers never cross threads.
#[pyclass(unsendable)]
pub struct Fleet {
    deck: Deck,
    num_envs: usize,
    workers: Vec<Worker>,
    // Assembled per-env state (Fleet-owned copies from the last batch — the workers own the slots).
    globals: Vec<f32>,
    obs_flat: Vec<(&'static str, bool, Vec<f32>, Vec<i64>)>,
    mask: Vec<u8>,
    seat: Vec<i32>,
    num_legal: Vec<i32>,
    terminal: Vec<i32>,
    requests: Vec<String>,
    summaries: Vec<Option<(Option<i64>, u32, String)>>,
    stats: Vec<Vec<(&'static str, f64)>>, // per-env decision_stats from the last advance (not sticky)
    last_tick_us: u128,
}

impl Fleet {
    /// Map a global env index to `(worker slot, local index within its group)`.
    fn locate(&self, env: usize) -> (usize, usize) {
        for (w, worker) in self.workers.iter().enumerate() {
            if env >= worker.range.0 && env < worker.range.1 {
                return (w, env - worker.range.0);
            }
        }
        unreachable!("env {env} out of range")
    }

    /// One coordinated worker round-trip: hand each worker its `StepMsg`, collect all batches, and
    /// re-assemble. The workers step (and reset) their groups in parallel; the main blocks on `recv`.
    fn dispatch(&mut self, per_worker: Vec<StepMsg>) -> PyResult<()> {
        let died = || PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("a worker died mid-step");
        let t0 = Instant::now();
        for (worker, msg) in self.workers.iter().zip(per_worker) {
            worker.tx.send(Command::Step(msg)).map_err(|_| died())?;
        }
        let mut batches = Vec::with_capacity(self.workers.len());
        for (w, worker) in self.workers.iter().enumerate() {
            batches.push((w, worker.rx.recv().map_err(|_| died())?));
        }
        self.last_tick_us = t0.elapsed().as_micros();
        self.assemble(batches);
        Ok(())
    }

    fn empty_msgs(&self) -> Vec<StepMsg> {
        self.workers.iter().map(|_| StepMsg { steps: Vec::new(), resets: Vec::new() }).collect()
    }

    /// Collect one `GroupBatch` per worker and stitch them into the Fleet-owned contiguous buffers.
    fn assemble(&mut self, batches: Vec<(usize, GroupBatch)>) {
        // batches carry their worker index so order is deterministic regardless of arrival order.
        let mut ordered = batches;
        ordered.sort_by_key(|(w, _)| *w);
        self.globals.clear();
        for a in self.obs_flat.iter_mut() {
            a.2.clear();
            a.3.clear();
        }
        self.mask.clear();
        self.seat.clear();
        self.num_legal.clear();
        self.terminal.clear();
        self.requests.clear();
        self.stats.clear();
        for (_, b) in &ordered {
            self.globals.extend_from_slice(&b.globals);
            self.mask.extend_from_slice(&b.mask);
            self.seat.extend_from_slice(&b.seat);
            self.num_legal.extend_from_slice(&b.num_legal);
            self.terminal.extend_from_slice(&b.terminal);
            self.requests.extend(b.requests.iter().map(|s| s.to_string()));
            self.stats.extend(b.stats.iter().cloned());
            for (name, _is_int, fbuf, ibuf) in self.obs_flat.iter_mut() {
                match *name {
                    "bf_feat" => fbuf.extend_from_slice(&b.bf_feat),
                    "bf_grpid" => ibuf.extend_from_slice(&b.bf_grpid),
                    "hand_feat" => fbuf.extend_from_slice(&b.hand_feat),
                    "hand_grpid" => ibuf.extend_from_slice(&b.hand_grpid),
                    "stack_feat" => fbuf.extend_from_slice(&b.stack_feat),
                    "stack_grpid" => ibuf.extend_from_slice(&b.stack_grpid),
                    "decision_grpid" => ibuf.extend_from_slice(&b.decision_grpid),
                    "edges" => ibuf.extend_from_slice(&b.edges),
                    "choice_feat" => fbuf.extend_from_slice(&b.choice_feat),
                    _ => {}
                }
            }
        }
        // summaries: sticky — once an env ends its outcome is fixed (later batches carry Some again).
        let mut idx = 0;
        for (_, b) in &ordered {
            for s in &b.summaries {
                if s.is_some() {
                    self.summaries[idx] = s.clone();
                }
                idx += 1;
            }
        }
    }
}

#[pymethods]
impl Fleet {
    /// Build `num_envs` games of `deck` (seed `base_seed + i`), partitioned across `num_workers`
    /// pinned threads; each worker creates + advances its group and returns the first batch.
    #[new]
    #[pyo3(signature = (deck, num_envs, num_workers = 1, auto_pass = true, base_seed = 0))]
    fn new(deck: &str, num_envs: usize, num_workers: usize, auto_pass: bool, base_seed: u64) -> PyResult<Self> {
        let deck = Deck::parse(deck)
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("unknown deck {deck:?}")))?;
        let nw = num_workers.max(1).min(num_envs.max(1));
        let mut workers = Vec::with_capacity(nw);
        let mut init: Vec<(usize, GroupBatch)> = Vec::with_capacity(nw);
        // Contiguous, near-equal groups.
        let base = num_envs / nw;
        let rem = num_envs % nw;
        let mut start = 0usize;
        for w in 0..nw {
            let k = base + if w < rem { 1 } else { 0 };
            let range = (start, start + k);
            start += k;
            let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<Command>();
            let (res_tx, res_rx) = std::sync::mpsc::channel::<GroupBatch>();
            let seeds: Vec<u64> = (range.0..range.1).map(|i| base_seed.wrapping_add(i as u64)).collect();
            let handle = std::thread::spawn(move || {
                // Slots are created AND stepped here — the Session fibers never leave this thread.
                let mut slots: Vec<GameSlot> = seeds.iter().map(|&s| GameSlot::new(deck, s, auto_pass)).collect();
                let _ = res_tx.send(encode_group(&slots)); // initial batch
                while let Ok(cmd) = cmd_rx.recv() {
                    match cmd {
                        Command::Step(msg) => {
                            for (local, a) in &msg.steps {
                                let slot = &mut slots[*local];
                                if !slot.terminal {
                                    slot.apply(*a);
                                }
                            }
                            for (local, seed) in &msg.resets {
                                slots[*local] = GameSlot::new(deck, *seed, auto_pass);
                            }
                            if res_tx.send(encode_group(&slots)).is_err() {
                                break;
                            }
                        }
                        Command::Shutdown => break,
                    }
                }
            });
            init.push((w, res_rx.recv().map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("worker died at init"))?));
            workers.push(Worker { tx: cmd_tx, rx: res_rx, handle: Some(handle), range });
        }
        let obs_flat = obs::spec()
            .into_iter()
            .filter(|(n, ..)| *n != "globals")
            .map(|(n, _r, _c, is_int)| (n, is_int, Vec::<f32>::new(), Vec::<i64>::new()))
            .collect();
        let mut fleet = Fleet {
            deck,
            num_envs,
            workers,
            globals: Vec::new(),
            obs_flat,
            mask: Vec::new(),
            seat: Vec::new(),
            num_legal: Vec::new(),
            terminal: Vec::new(),
            requests: Vec::new(),
            summaries: vec![None; num_envs],
            stats: Vec::new(),
            last_tick_us: 0,
        };
        fleet.assemble(init);
        Ok(fleet)
    }

    fn num_envs(&self) -> usize {
        self.num_envs
    }
    fn action_dim(&self) -> usize {
        codec::ACTION_DIM
    }
    fn card_vocab(&self) -> Vec<u32> {
        self.deck.vocab()
    }

    /// Introspection for the bench: worker count, per-group sizes, and the last `submit`'s wall time
    /// (µs) — lets a reader tell stepping-bound (tick time grows with n_envs) from forward-bound.
    fn debug_stats<'py>(&self, py: Python<'py>) -> Bound<'py, PyDict> {
        let d = PyDict::new(py);
        d.set_item("num_workers", self.workers.len()).unwrap();
        let groups: Vec<usize> = self.workers.iter().map(|w| w.range.1 - w.range.0).collect();
        d.set_item("group_sizes", groups).unwrap();
        d.set_item("last_tick_us", self.last_tick_us as u64).unwrap();
        d
    }

    // ── per-env accessors (Fleet-owned copies from the last batch; used by the equivalence driver) ─
    fn seat(&self, i: usize) -> i64 {
        self.seat[i] as i64
    }
    fn request(&self, i: usize) -> String {
        self.requests[i].clone()
    }
    fn num_legal(&self, i: usize) -> usize {
        self.num_legal[i] as usize
    }
    fn env_mask(&self, i: usize) -> Vec<bool> {
        let ad = codec::ACTION_DIM;
        self.mask[i * ad..(i + 1) * ad].iter().map(|&b| b != 0).collect()
    }
    fn terminal(&self, i: usize) -> bool {
        self.terminal[i] != 0
    }
    fn summary(&self, i: usize) -> Option<(Option<i64>, u32, String)> {
        self.summaries[i].clone()
    }
    /// The `{field: value}` decision_stats env `i`'s slot finalized on the last advance, or `None` for
    /// a non-finalizing sub-step. Read right after the learner advance (before the pump's opponent
    /// advances overwrite it) so it's the LEARNER's decision — mirrors `MtgEnv.ext_take_stats`.
    fn decision_stats<'py>(&self, py: Python<'py>, i: usize) -> Option<Bound<'py, PyDict>> {
        let rec = &self.stats[i];
        if rec.is_empty() {
            return None;
        }
        let d = PyDict::new(py);
        for (k, v) in rec {
            d.set_item(*k, *v).unwrap();
        }
        Some(d)
    }

    /// Apply one factored action per env (full batch) and advance to the next decisions — the
    /// workers step **in parallel** (separate OS threads, pure-Rust engine work, no Python), the
    /// caller holds the GIL only while blocked on `recv`. Read via `tick`.
    fn submit(&mut self, actions: Vec<usize>) -> PyResult<()> {
        if actions.len() != self.num_envs {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "submit expects {} actions, got {}",
                self.num_envs,
                actions.len()
            )));
        }
        let mut per = self.empty_msgs();
        for (env, &a) in actions.iter().enumerate() {
            let (w, local) = self.locate(env);
            per[w].steps.push((local, a));
        }
        self.dispatch(per)
    }

    /// Selective advance for the self-play pump: apply `step_actions` to ONLY `step_envs` (the
    /// opponent-pending envs — learner envs listed nowhere keep their current decision, waiting for
    /// SB3), and restart `reset_envs` with `reset_seeds` (terminals → fresh games). One worker
    /// round-trip; envs are re-encoded so `tick` stays consistent. Clears reset envs' stale summaries.
    #[pyo3(signature = (step_envs, step_actions, reset_envs = vec![], reset_seeds = vec![]))]
    fn advance(&mut self, step_envs: Vec<usize>, step_actions: Vec<usize>,
               reset_envs: Vec<usize>, reset_seeds: Vec<u64>) -> PyResult<()> {
        if step_envs.len() != step_actions.len() || reset_envs.len() != reset_seeds.len() {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "advance: step_envs/step_actions and reset_envs/reset_seeds must be equal length",
            ));
        }
        let mut per = self.empty_msgs();
        for (&env, &a) in step_envs.iter().zip(step_actions.iter()) {
            let (w, local) = self.locate(env);
            per[w].steps.push((local, a));
        }
        for (&env, &s) in reset_envs.iter().zip(reset_seeds.iter()) {
            let (w, local) = self.locate(env);
            per[w].resets.push((local, s));
            self.summaries[env] = None; // fresh game — drop the finished game's outcome
        }
        self.dispatch(per)
    }

    /// Cross the current batch to Python once as bytes: `{obs arrays, "mask"(u8), "seat"/"num_legal"/
    /// "terminal"(i32)}`. Python `np.frombuffer(...).reshape(num_envs, ...)` (shapes from
    /// `PyGame.obs_spec()` + `action_dim`).
    fn tick<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let out = PyDict::new(py);
        out.set_item("globals", PyBytes::new(py, as_bytes_f32(&self.globals)))?;
        for (name, is_int, fbuf, ibuf) in &self.obs_flat {
            let bytes = if *is_int { as_bytes_i64(ibuf) } else { as_bytes_f32(fbuf) };
            out.set_item(*name, PyBytes::new(py, bytes))?;
        }
        out.set_item("mask", PyBytes::new(py, &self.mask))?;
        out.set_item("seat", PyBytes::new(py, as_bytes_i32(&self.seat)))?;
        out.set_item("num_legal", PyBytes::new(py, as_bytes_i32(&self.num_legal)))?;
        out.set_item("terminal", PyBytes::new(py, as_bytes_i32(&self.terminal)))?;
        Ok(out)
    }
}

impl Drop for Fleet {
    fn drop(&mut self) {
        for w in &self.workers {
            let _ = w.tx.send(Command::Shutdown);
        }
        for w in &mut self.workers {
            if let Some(h) = w.handle.take() {
                let _ = h.join();
            }
        }
    }
}

// Reinterpret POD slices as bytes for the one-crossing transfer (native-endian, matched by the
// Python side's np.frombuffer dtype). Plain `#[repr]` scalars — no external crate.
fn as_bytes_f32(s: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}
fn as_bytes_i64(s: &[i64]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}
fn as_bytes_i32(s: &[i32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}
