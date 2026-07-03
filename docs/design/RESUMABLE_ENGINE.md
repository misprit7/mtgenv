# The Resumable Engine: Pull, Don't Push

> **Status:** Design proposal (M3, Phase A). Not yet implemented — awaiting lead sign-off before
> Phase B. Source of truth once built: `crates/mtg-core/src/priority.rs` (the driver + `ask`
> seam) and a new `session` module.
> **Read first:** `docs/design/AGENT_INTERFACE.md` (the decision boundary this preserves),
> `docs/design/WHITEBOARD_MODEL.md` §"Naps" (MTGA's GRE already suspends/resumes — this is that,
> made explicit), `docs/plans/GYM_PLAN.md` §2.2-B + §8-M3 (the pre-agreed `resume`/`submit`
> sketch this fills in — **and re-scopes**, see §0.1).
> The Rust blocks below are **sketches**; the crate is the source of truth for exact field types.

---

## 0. Why

Today the engine is a **push** machine. `Engine::run_game()` runs a whole game on one call
stack, and every player choice is a *blocking* `self.ask(seat, req) → agents[seat].decide(view,
req)` made from wherever the engine happens to be — usually many frames deep inside nested loops
(priority round → cast → choose targets; agenda loop → resolve → effect runtime → "may?"). For
one game with an in-process agent this is fine. For a **fleet** of games driven by a Python RL
policy it is the bottleneck.

To make `decide` "return to Python," `mtg-py` today runs **each game on its own OS thread** and
shuttles every decision across a channel: `PyAgent::decide` (on the game thread) `send`s
`(seat, view, req)` and **blocks on `recv`**; Python's `PyGame::advance` blocks on the game→Py
channel with the **GIL released** (`py.allow_threads`), answers with an action `int`, which is
sent back to unblock the engine (`crates/mtg-py/src/game.rs`, `lib.rs`). One game = one thread +
two channel hops per decision + GIL serialization of the Python side. The measured verdict
(§5): **the per-decision boundary crossing is the dominant cost**, and naive process
parallelism (`SubprocVecEnv`) is *worse* than serial because the big Dict-observation IPC
dominates (**0.6 vs 14 games/s/core**).

We want the **inverse**: a **pull** primitive — advance to the next decision and **return**,
apply a response, continue — with "where we are in the game" captured explicitly. Then a fleet
of games is stepped **GIL-free inside Rust** (thread-pinned groups or rayon), Python sees only
**batched tensors**, and the per-decision boundary collapses from *thread-wake + two channel
hops* to *a function return*.

### 0.1 Scope — throughput and cleanliness ONLY (a re-scope of GYM_PLAN M3)

The **only** justifications are **throughput** and **architectural cleanliness**. The lead's
directive (user-set) is explicit: **there will never be engine-backed tree search** (a learned
MuZero-style search needs no engine support). Therefore this design **does not** target
state-snapshot / clone-for-search. If cheap cloning falls out for free, fine — but it **must
not drive any decision**, and in fact the recommended strategy (§2-B) deliberately does *not*
provide it.

> ⚠️ **Divergence from the committed plan — the lead must reconcile.** `GYM_PLAN.md` §2.2-B and
> §8-Milestone-3 currently frame this work around MCTS: *"the natural substrate for clone-based
> MCTS," "use `clone`/`snapshot` for AlphaZero-style rollouts," exit = "clone-based search
> demonstrated."* Under the new directive those goals are **cut**. The `PyGame.snapshot` /
> `restore` / `clone_game` stubs (`crates/mtg-py/src/lib.rs:242-256`) **stay stubbed** — they
> are no longer a deliverable. GYM_PLAN M3's exit criteria should be rewritten to *throughput
> (GIL-free fleet stepping) + the `PyAgent` thread/channel removal*, dropping the search line. I
> have **not** edited GYM_PLAN (tracker ownership); flagging for the lead.

The single `Agent` decision boundary LAW is untouched: `DecisionRequest` / `DecisionResponse` /
`PlayerView` stay the entire vocabulary. This doc changes *how the engine reaches* a decision
(blocking-call → return-and-resume), not *what* a decision is. Existing backends —
`GreSessionAgent`, `RandomAgent`, the scripted test agents, the `PyAgent` — keep working; the
blocking `Agent` trait becomes a **thin driver loop** over the new primitive, not a parallel
code path.

---

## 1. Inventory — every suspension site and its context

### 1.1 The single seam

Every player decision in `mtg-core` funnels through **one** method:

```
priority.rs:2322  fn ask(&mut self, p: PlayerId, req: &DecisionRequest) -> DecisionResponse {
                      let mut view = self.view_for_seat(p);
                      self.reveal_request_objects(&mut view, req);
                      self.agents[p.0 as usize].decide(&view, req)   // ← the ONE call-out
                  }
```

Nothing else consults an agent. This is the load-bearing fact of the whole design: **we only
have to change how `ask` hands control back, in one place.** Every site below reaches the agent
*through* `ask`.

### 1.2 The ~24 call sites, by suspension context

Counts: `whiteboard.rs` 9, `priority.rs` 12 (2 pre-game, 1 is a test helper), `combat/mod.rs` 3.

| Site (file:line) | Context / `DecisionRequest` | Reached via | In a loop/fixpoint? |
|---|---|---|---|
| **Pre-game** |
| priority.rs:516 `choose_starting_player` | ChooseStartingPlayer | start_game | no |
| priority.rs:564/598 `run_mulligans` | Mulligan; SelectCards(bottom) | start_game | **yes** (mulligan rounds `loop`) |
| **Top-level priority** |
| priority.rs:855 `priority_round` | Priority (cast/activate/play/pass) | take_turn→run_step→priority_round | **yes** (`loop`, cursor `idx`/`passes`) |
| **Mid-cast (the deep nest)** |
| priority.rs:1164/1220 `activate_ability` | ChooseTargets; cost | perform_priority_action | inside priority `loop` |
| priority.rs:1296/1384 crew / sacrifice cost | SelectCards | perform_priority_action | " |
| priority.rs:1491/1546 `cast_spell` | ChooseNumber(X); ChooseTargets | perform_priority_action | " |
| (cast_spell → `choose_modes`) | ChooseModes | perform_priority_action | " |
| **Trigger targeting** |
| priority.rs:2231 push-triggered-ability | ChooseTargets (trigger, 603.3d) | run_agenda | **yes** (`run_agenda` `loop`) |
| **Combat** |
| combat/mod.rs:156/270/508 | DeclareAttackers; DeclareBlockers; AssignCombatDamage | run_step(combat) | per-source for damage |
| **Mid-resolution effect runtime (whiteboard.rs)** |
| whiteboard.rs:204 `interpret` | Confirm(MayEffect) | resolve_top→resolve_effect | **yes** (in run_agenda/resolve_to_stable) |
| whiteboard.rs:271/326/387 search/discard/surveil | SelectCards; ArrangeCards | " | yes |
| whiteboard.rs:473 counter | (counter choice) | " | yes |
| whiteboard.rs:507 `select_for_each` | SelectCards | " — **can nest inside a ForEach loop** | yes |
| whiteboard.rs:560 `add_mana` | ChooseColor / mana | " | yes |
| whiteboard.rs:1009/1116 replacements | ChooseReplacement (616.1f); Confirm(PayToPrevent) | rewrite pass | yes |

**Read of the inventory.** Suspension points are (a) few (~24) and (b) *all* routed through one
seam — but (c) scattered across **arbitrary nesting depth** and, decisively, **inside four
distinct loop/fixpoint structures** whose loop-local cursor state (mulligan round, priority
`idx`/`passes`, `run_agenda` re-drain, `resolve_to_stable`, ForEach index) would each have to be
reified and made re-enterable if we captured continuations by hand. The **mid-cast** nest (cast
→ modes → X → targets) and the **mid-resolution** nest (agenda → resolve → interpret → "may?")
are the deep ones. Illustration — `priority_round` (priority.rs:818) is a `loop` carrying
`idx`/`passes`/`iters` across the `self.ask` at line 855; a by-hand continuation must save all
three and re-enter mid-iteration. **This shape is the single most important input to the
strategy choice.**

---

## 2. Strategy options

Three ways to make a deeply-nested blocking `ask` return-and-resume. Judged on: code churn,
**risk of behavior drift** (≈300 tests must stay green — many pin exact turn traces via
expect-test), per-game memory, Send/parallelism (the fleet is the endgame), and how trivially
the blocking `Agent` trait collapses to a driver.

### Option A — explicit continuation / state machine in `GameState`

Reify each suspension as an enum variant carrying the locals to resume it, plus a resume
dispatch; restructure every enclosing loop to re-enter from a saved cursor.

- **Churn:** very high (~24 sites × deep nesting; every mid-cast/mid-resolution local becomes
  explicit state). **Drift risk:** very high — you rewrite the exact control flow the turn-trace
  snapshots pin. **Memory:** small, explicit, **serializable** (its one real edge).
  **Send/parallel:** trivially Send. **Blocking wrapper:** becomes a driver, but only after a
  full rewrite.
- **Verdict:** its serializable-continuation upside **is exactly the snapshot-for-search we've
  ruled out** (§0.1). Remove that and A is all cost, no unique benefit. **Rejected** (kept only
  as the fallback if B's spike fails).

### Option B — stackful coroutine (one fiber per game) — **RECOMMENDED**

Run the game body inside a **stackful coroutine** (a *fiber*: its own small stack,
cooperatively scheduled — **not** an OS thread). `ask` becomes a **`yield`**: it suspends the
fiber, hands the `DecisionRequest` (+ `PlayerView` + seat) out, and resumes with the
`DecisionResponse`. **The game logic is not touched at all** — `cast_spell`, `run_agenda`,
`priority_round`, `combat`, the whole effect runtime stay direct-style; the native call stack
*is* the continuation.

- **Churn:** low, concentrated — split `agents` out of the core (§3.1), change `ask`'s tail from
  "call `decide`" to "yield," add the `Session`/`resume`/`submit` primitive + a driver loop,
  port `mtg-py`. **Zero** changes to game-logic bodies.
- **Drift risk:** **near-zero** — the ≈300 tests exercise byte-identical code paths; only the
  final dispatch inside `ask` differs. This is the decisive advantage.
- **Memory:** one stack per suspended game, tunable fixed size. Engine recursion is shallow and
  bounded (no deep recursion), so 32–64 KiB/fiber is plausible (must **measure** max depth). 32
  envs ≈ 2 MB; 1024 envs × 64 KiB ≈ 64 MB — a non-issue at realistic fleet sizes.
- **Send/parallel — the one real risk.** A fiber's suspended stack isn't serializable (fine — no
  search) and cheap cloning does **not** fall out (a fiber has no clone — perfectly aligned with
  §0.1). Whether the coroutine is **`Send`** (needed to move a game between rayon workers)
  depends on the crate. **RESOLVED in M3.0 (§6.5):** `corosensei` is `!Send` by design but its
  docs sanction a manual `unsafe impl Send` when the stack data is `Send`; the spike moves a live
  suspended fiber across threads that way → **rayon fleet is viable**. The thread-pinned-groups
  model (§3.4) remains a zero-`unsafe` fallback needing no `Send` at all.
- **Blocking wrapper:** collapses to `let mut r=None; loop { match sess.step(r) { Suspend{seat,
  view,req} => r=Some(agents[seat].decide(&view,&req)), Done(o)=>break o } }`. Exactly the
  mandated shape.
- **Cons to own:** a new dependency with `unsafe` internals (audited, widely used — `corosensei`
  underlies Wasmtime's stack switching); must confirm it **builds offline** here (workspace deps
  are currently only serde/thiserror/expect-test); one small contained `unsafe` at the seam (a
  raw pointer to the fiber's `Yielder`, valid only while the fiber runs — §3.2). Real but
  bounded.

### Option C — agenda-level restructure + pending-decision cursor (hybrid)

`WHITEBOARD_MODEL.md` §2.2 already models the engine as an ordered **agenda** ("naps"). Where a
decision is *already* agenda-mediated, surface it as an agenda item with a resume cursor; use A
for the rest.

- **Reality check:** the agenda mediates *internal* processing (trigger collection, rewrite
  pass, SBAs). The **mid-cast** asks (modes/X/targets) and **combat** declarations and the
  **priority** ask happen *outside* the agenda, in `cast_spell` / `priority_round` / `combat`.
  So C still must solve exactly the hard nests that make A expensive — it degenerates to "A for
  the hard 60%, agenda for the easy 40%": two mechanisms, *more* total surface. **Rejected as
  primary** (but see §6 — the agenda remains the right home for *internal* resumption and
  composes fine with B).

### Decision

**Option B (stackful coroutine).** Only B keeps the turn-trace suite green *by construction*
(identical code paths), makes the blocking trait a trivial driver, and needs no serializable
continuation we have no use for. Its one genuine risk (Send / offline-buildable crate) is
isolated and de-risked by the first-milestone spike **and** by a fleet model that doesn't
require Send. Fall back to **A** (staged identically) only if the spike fails on *both* Send and
offline-build.

---

## 3. Target architecture

### 3.1 Split the engine into a `Send` core + a driver

`Engine` today holds game state **and** `agents: Vec<Box<dyn Agent>>` (priority.rs:172). The
agents (Python/socket handles, `dyn Agent` — not `Send`) are the *only* thing that must not live
inside a fiber. Split:

```
EngineCore   // everything except agents: state, card_db, rng, replay*, searched/foreach,
             // stops (Arc<Mutex> — already Send). Owns ALL game-logic methods. `ask` YIELDS.
             // Target Send: bound replay_sink `+ Send`, or move the live sink to the driver.

Session      // the RESUMABLE PRIMITIVE: owns a coroutine wrapping EngineCore::run_game.
             //   resume()      -> Step        // advance to next decision; calls NO Agent
             //   submit(resp)                 // stash the response for the next resume()
             // Step = Decision { seat, view, request } | GameOver { outcome }
             // GIL-free; Send target. This is what a fleet holds.

Engine       // BACK-COMPAT blocking API: { session: Session, agents: Vec<Box<dyn Agent>> }.
             // run_game() = drive `session` with `agents`. Signature UNCHANGED.
```

`Engine::new(state, agents)` and `Engine::run_game()` keep their exact signatures, so
**mtg-cli, mtg-gre-server, and every test call site are untouched.** The GRE server is a pure
`Engine::new(...).run_game()` blocking-agent path (`crates/mtg-gre-server/src/driver.rs:95-97`)
and `GreSessionAgent` does its socket protocol inside `decide`; it neither knows nor cares that
`run_game` now drives a coroutine underneath.

> **API naming.** GYM_PLAN §2.2-B pre-agreed `resume() -> Step` + `submit(response)` on the
> engine. We keep those verbs on `Session` (the resumable primitive). One deliberate addition to
> the pre-agreed `Step::Decision { seat, request }`: it also carries **`view: PlayerView`**,
> because `obs::encode` needs it and, once suspended, the state lives inside the fiber and can't
> be borrowed out — the view must be *yielded by value*. This costs nothing extra: `ask` already
> builds the view today.

### 3.2 What `ask` becomes

```
// EngineCore, running INSIDE the fiber:
fn ask(&mut self, p, req) -> DecisionResponse {
    let mut view = self.view_for_seat(p);           // unchanged — already built here today
    self.reveal_request_objects(&mut view, req);
    match self.yielder {                             // Some ⇒ in a fiber; None ⇒ legacy direct call
        Some(y) => unsafe { (*y).suspend(Step::Decision { seat: p, view, request: req.clone() }) },
        None    => self.agents[p.0 as usize].decide(&view, req),   // ← kept for direct-call unit tests
    }
}
```

The `Yielder` is corosensei's per-fiber handle, valid only while the fiber runs; we stash a
`*const Yielder<…>` in `EngineCore` at fiber start and deref it in `ask`. This is the **one**
`unsafe` line; it is sound because `ask` only ever executes while its owning fiber is running.
No other method changes.

> **On the "single path" goal.** The ≈40 unit tests that call sub-entry points directly
> (`e.cast_spell(...)`, `e.resolve_top(...)`, then assert on state) supply a blocking agent and
> spin up no fiber. The `None` branch keeps them green with **zero churn**. This is **not** a
> duplicated game-logic path — it is a one-line branch at the single seam; all *rules* logic
> stays singular. Full-game play (run_game, gym, GRE) always flows through the fiber+driver. We
> *can* later migrate those tests onto a `drive(|core| …)` helper and delete the branch, but
> that churn buys nothing now.

### 3.3 The primitive and the blocking driver

```
enum Step { Decision { seat: PlayerId, view: PlayerView, request: DecisionRequest },
            GameOver { outcome: Outcome } }

impl Session {
    fn start(state /*, config; NO agents */) -> Session   // build fiber; don't run yet
    fn resume(&mut self) -> Step                          // run to next Decision / GameOver
    fn submit(&mut self, resp: DecisionResponse)          // stash resp for the next resume()
    fn into_state(self) -> GameState                      // reclaim finished state (fiber returns core)
}

// The blocking Agent trait, now a thin driver (this IS Engine::run_game):
loop {
    match self.session.resume() {
        Step::Decision { seat, view, request } =>
            self.session.submit(self.agents[seat].decide(&view, &request)),
        Step::GameOver { outcome } => break outcome,
    }
}
```

`observe`/event streaming: events are emitted inside the fiber. Either buffer them in
`EngineCore` and drain into the driver alongside each `Step` (fan out to `agent.observe`), or
keep a `+ Send` observer callback in the core. Detail for M3.2. The GRE/replay live-sink already
runs "on the game thread," which is now the fiber — semantics unchanged.

### 3.4 Fleet stepping (the endgame)

Two models; pick per the spike:

- **Thread-pinned groups (safe default; no `Send` fiber needed).** `std::thread::scope` with P
  workers; partition N sessions into P groups, each **created and stepped only on its owning
  thread** (fibers never migrate). For batched inference, each micro-tick: every thread
  `resume`s its games to their next `Decision`, writing obs (`obs::encode`, already Rust) into a
  shared `[N × obs_dim]` buffer at fixed indices; barrier; Python runs **one** batched forward;
  scatter actions into per-session response slots; every thread `submit`s + `resume`s; barrier;
  repeat. `Done` games reset in place. Only the obs buffer + response slots are shared (disjoint
  `&mut [T]` slices — safe); never a fiber across threads.
- **rayon over `Vec<Session>` (simpler; needs `Send` fibers).** If the spike shows the coroutine
  is soundly `Send`, `par_chunks_mut` the sessions; work-stealing balances uneven game lengths.
  Preferred *iff* Send is clean.

Either way, **Python never touches per-game control flow.** It calls one Rust entry
(`fleet.step(actions) -> (obs, masks, rewards, dones)`); all envs advance GIL-free in Rust;
Python does tensor work only. This **replaces** today's `BatchedSelfPlayVecEnv` Python-side pump
(`python/mtgenv_gym/batched_selfplay.py`), which loops over envs in Python to gather
batchable opponent decisions — the fleet does that grouping in Rust, and can **group-and-pad by
decision-kind** to fix the effective-batch desync (auto-pass currently shrinks the opponent
batch to ~9 at n=64, §5).

---

## 4. Migration plan (green at every commit)

`cargo test -p mtg-core` (≈300 tests) passes at **every** listed commit. Coordinated with
sos-cards (cards/effects/whiteboard are theirs mid-flight; my zone is priority/turn/stack/agent
seam + new files). Any restructure of a file they're editing goes through the lead first.

- **M3.0 — Spike (throwaway/bench-only).** Add the chosen coroutine crate; prove (a) it
  **builds offline** here; (b) yield → resume-with-input → return round-trips; (c) it survives
  moving across a thread boundary (Send) *or* confirm the thread-pinned model. No engine change.
  **Go/no-go on Option B.** *(No-go ⇒ pivot to Option A, same skeleton below.)*
- **M3.1 — Core/agent split, behavior-preserving.** Introduce `EngineCore` (state + all logic);
  make `Engine = { core, agents }` delegating to it; `ask` still calls agents. Pure refactor, no
  coroutine yet. **All tests green.** (Largest mechanical diff, zero behavior change — the
  unchanged test suite is the net.)
- **M3.2 — Introduce `Session` + `resume`/`submit`; drive `run_game` through it.** `ask` yields
  when a yielder is present (§3.2); `Engine::run_game` becomes the driver loop; the `None` branch
  keeps direct-call unit tests working. Full-game blocking play now flows through the primitive.
  **All tests green** (turn-trace snapshots unchanged — same code paths). Wire `observe`/events.
- **M3.3 — Port `mtg-py` to `resume`/`submit`; add the Rust fleet.** Drop `PyGame`'s per-game
  thread + `GameConn` channels + `PyAgent` (`crates/mtg-py/src/game.rs`); `PyGame` becomes a
  thin `Session` wrapper (kills the `unsendable` pin and the `py.allow_threads` blocking recv).
  Add a Rust `Fleet` stepper (thread-pinned groups first; rayon if Send is clean) writing
  batched obs via the existing `obs.rs`/`codec.rs`. Keep the old path behind a flag until the new
  one matches outcomes on a fixed seed set.
- **M3.4 — Benchmark + delete the old pump.** Measure vs baseline (§5); remove thread-per-game
  and the Python-side `BatchedSelfPlayVecEnv` pump once parity + speedup are confirmed.
  mtg-gre-server: **no change** (still `Engine::new().run_game()`).

---

## 5. Performance model

**Baseline (what's actually recorded).** The lead-cited "7.3k decisions/s" and "584–717 fps @
32 envs" are **fresh runs**, not persisted — `benchmark.py` prints `decisions/s` for
single-thread random rollout, and `584–717 fps` is SB3's `time/fps` rollout scalar (neither
defaults to 32 envs; that's `--n-envs 32`). The **committed** baselines:

| Metric | Value | Source |
|---|---|---|
| Random self-play, single thread | **~10k–24k decisions/s/thread** (M0) | WORKLOG.md:1062 |
| No-NN env-step ceiling (engine + PyO3 + obs) | **~2.7k env-steps/s** | throughput.py, WORKLOG.md:518 |
| Raw engine, no NN | **~54 games/s/core** | WORKLOG.md:782, GYM_PLAN §6 |
| Self-play `DummyVecEnv(8)` | **~14 games/s/core** | WORKLOG.md:782 |
| Self-play `SubprocVecEnv` | **~0.6 games/s/core** (IPC-bound!) | WORKLOG.md:782 |
| Batched opponent inference | **1.2–1.4× at n=32–64** (eff. batch ~9; auto-pass desync) | WORKLOG.md:514-518 |
| Policy forward (`bench_infer`) | batch-1 0.57 ms ≈ batch-64 0.66 ms | WORKLOG.md:514 |

**Where the time goes (diagnosis).** Two facts pin the bottleneck: (i) `SubprocVecEnv` (real OS
parallelism) is **~23× *slower*** than serial `DummyVecEnv` — the multi-hundred-KB Dict-obs IPC
per step swamps any parallelism win; and (ii) the policy forward is **nearly batch-size-flat**
(0.57 → 0.66 ms from batch 1 → 64), so the GPU is idle-waiting, not saturated. The cost is the
**per-decision boundary**: thread-wake + two `mpsc` hops + GIL reacquire + per-env PyO3 obs
marshaling, paid once per decision (~75 decisions/game).

**Where the resumable fleet wins.**
1. **Boundary cost: µs → ns.** A coroutine `yield`/`resume` is a stack-pointer swap (~ns); it
   replaces a thread-wake + two channel hops (~µs) *per decision*. At ~75 dec/game this is the
   direct throughput lever.
2. **GIL-free CPU scaling.** Games share no state; P cores step ~N/P games in parallel in Rust.
   The env-step side scales ~linearly with cores — the part that `SubprocVecEnv` failed to
   deliver because IPC, not compute, was the wall.
3. **One PyO3 crossing per micro-tick, not per env.** Obs already encode in Rust (`obs.rs`);
   the fleet writes directly into a batched `[N × D]` buffer, so Python receives *one* tensor
   batch per tick instead of N marshaled Dict-obs. This is what kills the IPC term that sank
   `SubprocVecEnv`.
4. **Bigger effective batch → real GPU use.** Envs become KB-scale structs (no OS thread), so
   1000s of envs are feasible; combined with Rust-side group-and-pad by decision-kind, the
   policy batch grows well past today's ~9-effective, and (per `bench_infer`) the GPU serves it
   at nearly the same latency — near-free throughput headroom.

**Expected outcome (model, not a promise).** The env-step side stops being the bottleneck: raw
engine is ~54 games/s/core × P cores with negligible boundary overhead, so the ceiling moves
onto the policy forward/backward, which batching keeps cheap. Concretely we should expect the
no-NN env-step rate to rise from ~2.7k/s toward `cores × (per-core engine rate)` with the
boundary term removed, and training fps to become GPU-bound at a **multiple** of the 584–717
baseline — the exact multiple set by core count and policy size, which the M3.4 benchmark will
measure. The honest floor claim: **removing the per-decision boundary and the Dict-obs IPC
recovers the parallelism `SubprocVecEnv` threw away, without its IPC penalty.**

---

## 6.5 M3.0 spike results (measured — `crates/mtg-coro-spike`, throwaway, deleted in M3.4)

All go-signals green. Spike = 6 passing tests; reproduce with `cargo test -p mtg-coro-spike --
--nocapture`.

- **Crate & offline build:** `corosensei = "=0.2.2"` (pinned) fetches and compiles here; deps are
  just `scopeguard` (+ Windows-only shims, unused on Linux). Minimal footprint.
- **API:** `Coroutine::new(|yielder, input| …)`, `yielder.suspend(y) -> next_input`, `resume(input)
  -> CoroutineResult::{Yield,Return}` — exactly the `ask`-yields shape the design assumes.
- **Real engine in a fiber:** a full random self-play game runs to completion on a fiber stack;
  the engine is oblivious to being a fiber (no game-logic change needed).
- **Fiber stack size — MEASURED 42 KiB worst-case.** Painted each fiber's stack with a sentinel
  and scanned the high-water mark over **125 games** (5 preset decks × 25 seeds). Worst was
  **42,080 B (~42 KiB)** on `selesnya` (the trigger/replacement-heavy deck — so triggers *are*
  exercised). No unbounded recursion, so this is a tight bound. **Recommendation: 256 KiB per
  fiber** (~6× headroom to cover deeper hand-constructed cascades the random population may not
  reach). Memory: 512 fibers × 256 KiB = 128 MB; 4096 × 256 KiB = 1 GB — vs corosensei's 1 MiB
  default (512 → 512 MB), so the explicit small size is what keeps a large fleet affordable.
  corosensei installs a guard page → an overflow is a fault, so size conservatively; treat a
  fiber overflow like a panicked game (§ below).
- **Send — `!Send` by design, but manual `impl` is sanctioned & works.** `corosensei::Coroutine`
  is deliberately `!Send` (a `PhantomData<*mut ()>`): the crate *can't prove* the suspended stack
  is `Send`. Its docs explicitly permit a manual `unsafe impl Send` when all stack data is `Send`.
  Our fiber stack holds only `EngineCore` + engine locals (all `Send`; the non-`Send` agents live
  in the driver), so `unsafe impl Send for Session {}` is sound and sanctioned. The spike moves a
  **live, suspended** fiber across a thread boundary and resumes it correctly, and runs a real
  engine game on a worker thread. ⇒ **rayon fleet is viable** (not only thread-pinned groups); the
  invariant to enforce in M3.1 is "`EngineCore: Send`" (keep agents out of the core).
- **Panic isolation — confirmed.** A panic inside a fiber body propagates out of `resume`;
  wrapping `resume` in `catch_unwind` contains it. A 5-fiber "fleet" where fiber #2 panics (stand-in
  for sos-cards' unwired-leaf `debug_assert`) yields `[Ok, Ok, Err, Ok, Ok]` — the panicked game
  reports a terminal error, the others finish. (Requires the default unwinding panic strategy — no
  `panic=abort` in any Cargo.toml, confirmed. The fleet driver must `catch_unwind` each `resume`
  and mark a caught game terminal-error.)

### Unsafe surface (for the doc record, per lead)

Option B introduces exactly two small, contained `unsafe` sites, both justified above:
1. `unsafe impl Send for Session` — sound iff `EngineCore: Send` (agents excluded). Enforce with a
   `fn _assert_send<T: Send>(){}` static check on `EngineCore` in M3.1.
2. the `*const Yielder` deref in `ask` (§3.2) — sound because `ask` only runs while its fiber is
   live. Plus corosensei's own (audited, wasmtime-grade) `unsafe` for stack switching.

## 6. Open questions / notes

- ~~**Coroutine crate + offline build**~~ **RESOLVED (M3.0):** `corosensei = "=0.2.2"`, builds
  offline, `!Send`-by-design but manual `unsafe impl Send` is sound & sanctioned (§6.5).
- ~~**Fiber stack size**~~ **RESOLVED (M3.0):** measured 42 KiB worst-case; use **256 KiB/fiber**
  (§6.5). Re-measure if a future card adds a deep effect-tree cascade.
- **Event/`observe` delivery** across the fiber boundary (§3.3) — buffer vs `+ Send` callback.
- **Agenda + coroutine coexistence** — the agenda (WHITEBOARD_MODEL §2.2) stays the home for
  *internal* ordered processing; the coroutine handles only *player-decision* suspension. They
  compose; neither subsumes the other.
- **Determinism/replay** — the seeded RNG lives in `EngineCore`; stepping order is unchanged, so
  replays and expect-tests stay bit-identical.
- **GYM_PLAN reconciliation (§0.1)** — the lead should re-scope GYM_PLAN M3 off MCTS/clone; the
  `snapshot`/`restore`/`clone_game` stubs stay stubbed.
