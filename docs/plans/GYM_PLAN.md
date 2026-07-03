# GYM_PLAN — Python RL Training Environment for the mtgenv Rust MTG Engine

Status: **current spec, no gym code yet** (engine is at milestone 5; the gym is milestone 6).
The decision boundary it wraps is **already built and stable** — `crates/mtg-core/src/agent.rs`
(the `Agent` trait, `DecisionRequest`/`DecisionResponse`, `PlayerView`, `RandomAgent`) — so this
plan specs against the *real* types, not a sketch.

**Read first (sources of truth, do not duplicate here):**
- `docs/design/AGENT_INTERFACE.md` — the one decision boundary. **The RL policy is just another
  `Agent` backend behind this seam.** §3/§4 are the canonical request/response set; this plan
  references them rather than re-specifying.
- `docs/plans/ENGINE_PLAN.md` §7 — determinism, cheap clone/snapshot, hidden-info masking, and the
  "tree-search readiness" note (the resumable-step API the gym needs).
- `crates/mtg-core/src/agent.rs` — the implemented boundary (`PlayerView`, the 21 `DecisionRequest`
  variants, the engine-enumerated legal options = the action mask).
- `crates/mtg-core/src/priority.rs` — the `Engine`, `legal_actions()`, and the **callback-driven**
  control flow (`decide()` only ever sees a masked `PlayerView`), plus the Arena-profile auto-pass /
  stops (`StopConfig`, `set_arena_auto_pass`) that already elide trivial priority windows.
- `crates/mtg-gre-server/src/{session.rs,options.rs}` — **the existing precedent for everything this
  plan needs.** `GreSessionAgent` already bridges the synchronous engine to an *external* decider
  over channels (the `PyAgent` pattern); `options.rs` already projects every `DecisionRequest` into a
  flat option list + maps a flat selection back (the proto-action-space).

Goal: a high-throughput Gymnasium environment that wraps `mtg-core` to train an MTG agent in
Python + PyTorch via self-play. The same `Agent` boundary serves three interchangeable drivers — a
native Rust scripted/heuristic AI, **this Python RL policy**, and the web/MTGA GRE client — with no
engine changes when you swap them (the project's "easy switch" law, AGENT_INTERFACE §0).

Lessons carried forward: the abandoned `../../from-scratch/mtgai` got stuck wrapping an existing
engine's internals (XStream deserialization, tight coupling). The clean-room Rust engine avoids
this entirely — **we own the decision boundary end to end**, and it is already implemented and
serde-clean.

---

## 1. Architecture

Five layers, each with a single responsibility and a stable interface to the next:

```
┌──────────────────────────────────────────────────────────────────────────┐
│ L5  PyTorch training (self-play, MaskablePPO now / AlphaZero-style later)   │
│     policy/value nets, replay buffer, league/opponent pool                  │
└───────────────▲───────────────────────────────────────────┬───────────────┘
                │ batched obs tensors + action masks          │ actions
┌───────────────┴───────────────────────────────────────────▼───────────────┐
│ L4  Gymnasium env  (python/mtgenv_gym, Python)                              │
│     MtgEnv(gym.Env): reset/step, obs encoder, action (de)coder, reward      │
│     Vector env over N in-process Rust games                                 │
└───────────────▲───────────────────────────────────────────┬───────────────┘
                │ (obs ndarray, mask, DecisionRequest digest) │ action index
┌───────────────┴───────────────────────────────────────────▼───────────────┐
│ L3  PyO3 bindings  (crates/mtg-py, maturin)                                 │
│     PyGame: new/reset/step_to_decision/apply/legal_mask/snapshot/restore    │
│     obs encoded in Rust into a numpy buffer (zero-copy); mask from options   │
└───────────────▲───────────────────────────────────────────┬───────────────┘
                │ Agent::decide(view, req) -> resp            │
┌───────────────┴───────────────────────────────────────────▼───────────────┐
│ L2  Decision / Agent boundary  (Rust trait `Agent`, IMPLEMENTED)            │
│     engine pauses at a choice point, builds a typed DecisionRequest with     │
│     ALL legal options enumerated, asks the seat's Agent, applies the         │
│     returned DecisionResponse. Backends: RandomAgent (done), GreSessionAgent │
│     (done, web/GRE), PyAgent (this plan), future MtgaClientAgent.            │
└───────────────▲───────────────────────────────────────────┬───────────────┘
                │ decision request                            │ chosen option
┌───────────────┴───────────────────────────────────────────▼───────────────┐
│ L1  Rules engine core  (mtg-core: headless, deterministic, no I/O)          │
│     GameState (Clone+serde), zones, turn/phase loop, priority, stack, SBAs,  │
│     combat, triggers, targeting, mana, layer system. Seeded RNG.            │
└────────────────────────────────────────────────────────────────────────────┘
```

L1 and L2 exist today. L3/L4/L5 are this plan's build (milestones, §8).

### The control loop (how a decision flows)

The engine is a **driver** that runs until a seat must choose, then asks that seat's `Agent`
(`crates/mtg-core/src/priority.rs::Engine::ask` → `agent.decide(view, req)`). For the gym we want the
inverse polarity — the env *pulls* requests, like a Gym `step` consumes one action — but the engine
is **synchronous and callback-driven** (`decide()` is total, blocking). §2 specifies how we invert
that without rewriting the engine.

```
                 L1 engine (one game thread)        L4 Gym env / L5 policy
   run loop: advance phases, put triggers on
   stack, check SBAs, grant priority …
        │ reach a choice point for seat S
        ▼
   build DecisionRequest (21 variants), e.g.
   Priority { actions:[legal…], can_pass } ◄── engine enumerates ONLY legal options
        │  Engine::ask(S, req) → agent.decide(view, req)
        ▼  PyAgent::decide blocks, hands (view, req) to the boundary
                                          │  PyGame.step_to_decision() returns
                                          │     (obs ndarray, mask, request digest)
                                          │  policy.forward(obs, mask) → action idx a
                                          ▼  PyGame.apply(a)
        ┌─────────────────────────────────┘  → DecisionResponse, unblocks decide()
        ▼
   validate + apply response, continue …
        ▼  … until terminal (a player loses / draw) → reward
```

Key property (AGENT_INTERFACE law #2): **the engine never asks an open-ended question.** Every
request ships the *complete enumerated legal set*; the policy only ever selects an index (or a
small structured payload) into that set. That is the whole reason RL gets action masking for free.

Engine core is **pure & deterministic**: same seed + same decisions ⇒ identical game. RNG is a
seeded `u64` field of `GameState` (`crates/mtg-core/src/rng.rs`), so clone/snapshot is trivial and
replay is exact — the basis for MCTS rollouts and differential/replay testing.

---

## 2. Rust ↔ Python boundary

**Recommendation: PyO3 + maturin, in-process, for the RL hot path.** A socket transport stays a
first-class alternative behind the same `Agent` trait — but it already exists for a *different*
consumer (the web/GRE client, `mtg-gre-server`), not the training loop.

### 2.1 Options compared

| Transport | Latency / decision | Serialization | Snapshot/clone | Fit |
|---|---|---|---|---|
| **PyO3 in-process** (recommend) | FFI call (~tens of ns–µs) | none for state; zero-copy numpy obs | trivial (clone the Rust `GameState`) | RL self-play hot path |
| Socket + JSON (`mtg-gre-server`) | ~50–500 µs + parse | full JSON per decision | hard (state in engine proc) | **web/GRE client & real MTGA client** — already built for that |
| Raw C FFI / cffi | low | manual unsafe C structs | manual | not worth it vs PyO3 |
| Shared-mem ring + flatbuffers | low | schema, zero-copy | medium | overkill now; revisit if distributed |

**Why PyO3 wins the inner loop.** A single MTG game is decision-heavy (most windows are "pass
priority"). At ≥10³–10⁴ decisions/game × ≥10³ games/s, a socket round-trip per decision is the
bottleneck. PyO3 keeps `GameState` in the Python process's address space: a decision is a plain
function call and the observation is written straight into a preallocated numpy buffer (zero-copy,
no serialization on the hot path). The socket path is correct and necessary for the *client* (a
human or the real MTGA client across a wire), but it would cap self-play throughput.

**Why the socket still matters (but not here).** `mtg-gre-server` is the GRE-protocol transport for
the web client and, later, the real MTGA client (`docs/plans/CLIENT_PLAN.md`). It is the same
`Agent` trait, a different implementation — proof the boundary is transport-agnostic. The gym does
**not** route through it.

### 2.2 The resumable-step API (the one real engine-coordination item)

Gym `step` semantics are *pull-based* (`reset → (obs, info)`, then `step(action) → (obs, …)`); the
engine is *push-based* (`Engine::run_game()` drives, calling `decide()` synchronously). We need
advance → return `(obs, request, mask)` → apply(action) → advance. **Two viable shapes; we
recommend (A) first, design for (B):**

- **(A) Thread + channel `PyAgent` (start here — zero engine changes).** This is exactly what
  `crates/mtg-gre-server/src/session.rs::GreSessionAgent` already does: run the synchronous game on
  its own OS thread; the seat's `Agent::decide` sends `(PlayerView, DecisionRequest)` over a channel
  and **blocks on `recv()`** for the response; the Python side, via PyO3, receives the request
  (`step_to_decision`), computes an action, and sends the `DecisionResponse` back (`apply`),
  unblocking the game thread. `PyGame` owns the thread + the two channel ends. This reuses a
  *proven* in-repo pattern and needs nothing from the engine. Cost: one OS thread per concurrent
  game and a blocking handoff (fine — the thread is idle while Python thinks; throughput comes from
  running many games, §5). The GIL is released around the blocking `recv`.

- **(B) A true resumable/`Coroutine` step API (engine addition — coordinate with `engine`).**
  ENGINE_PLAN §7 already flags this for MCTS: an engine that can *suspend* at a decision and return
  `(state, request)` to the caller, then be *resumed* with a response, without a thread. Shape to
  agree with `engine` (spec only — **do not build**):

  ```rust
  // Sketch — final shape owned by `engine`. The point is a single-threaded, re-entrant driver.
  pub enum Step {
      Decision { seat: PlayerId, request: DecisionRequest /* view via Engine::view_for */ },
      GameOver { outcome: Outcome },
  }
  impl Engine {
      /// Advance until the next decision or game end; does NOT call any Agent.
      pub fn resume(&mut self) -> Step;
      /// Supply the answer to the last `Decision` and continue on the next `resume`.
      pub fn submit(&mut self, response: DecisionResponse) -> Result<(), EngineError>;
  }
  ```

  This makes `PyGame` a thin, single-threaded wrapper (`resume`/`submit`), removes the per-game
  thread, and is the natural substrate for clone-based MCTS (snapshot at a `Decision`, branch,
  discard). It's a refactor of the priority/agenda driver to be re-entrant (today the recursion in
  `cleanup_step`/`priority_round` holds engine state on the Rust stack). **Required engine addition;
  flagged, not owned by gym.** Until it lands, (A) is fully sufficient and ships milestone 0–2
  unchanged; (B) is a drop-in `PyAgent`-internals swap later (the Python API doesn't change).

### 2.3 Concretely

- New workspace crate **`crates/mtg-py`** (PyO3 `cdylib`, built with **maturin**), depending only
  on `mtg-core` — a thin binding crate, **no engine logic** (repo bin/lib rule). Per ENGINE_PLAN §3
  crate layout.
- New **`python/mtgenv_gym/`**: `MtgEnv(gym.Env)`, the obs encoder glue, the action codec, a
  vectorized/async rollout collector, and a `MaskablePPO` entrypoint (SB3-contrib).
- The PyO3 surface is intentionally tiny (the `PyGame` handle):
  - `PyGame.new(config)` / `PyGame.reset(seed) -> (obs, mask, request_digest)`
  - `PyGame.step_to_decision() -> (obs, mask, request_digest)` — advance to the next decision
  - `PyGame.apply(action) -> None` — submit the decoded `DecisionResponse`
  - `PyGame.legal_mask() -> ndarray[bool]` — constant-width mask for the current decision
  - `PyGame.snapshot() -> bytes` / `PyGame.restore(bytes)` — serde round-trip of `GameState`
  - `PyGame.clone() -> PyGame` — cheap branch for MCTS (clone the Rust state)
  - `PyGame.outcome() -> Optional[winner]`
- **Observation encoding lives in Rust** (fast; writes into the numpy buffer). It reads the same
  `PlayerView` the boundary already produces — so hidden-info masking is inherited, not re-done (§3).

---

## 3. Observation space (PlayerView → tensors)

The observation is computed from the seat's **`PlayerView`** (`crates/mtg-core/src/agent.rs`), the
*already* information-filtered window: opponent hand is a count, library order is hidden, face-down /
unseen objects are `ObjView::Hidden` stubs. **Hidden-info masking is enforced once, in the engine's
`view_for(seat)` — the encoder never sees `GameState`, so a leak is structurally impossible** (the
same property the rules and a socket client rely on). This is the single most important reason the
observation is correct by construction.

A `gym.spaces.Dict` of fixed-shape tensors (pad + entity-mask everything variable-length):

- **Global scalars** (`Box`): `turn`; `phase` one-hot (12, the `Phase` enum); `active_player` /
  `priority_player` as relative flags (me/opp); per seat `life`, `poison`, `hand_count`,
  `library_count`, graveyard/exile sizes, `mana_pool` by color (from `ManaPool`), permanent counts;
  stack depth. (These are exactly the per-turn columns the `../magician` 17lands feature set tracks,
  made *current-state* instead of historical.)
- **Battlefield permanents** (`Box[MAX_PERM, F]`): one row per `view.battlefield` object. For
  `ObjView::Visible`, features from `CharacteristicsView` + status:
  - **`grp_id`** → the **card-embedding id** (an index into a learned embedding table — this is the
    Scryfall/oracle/printing id `CharacteristicsView.grp_id` already carries; it is how the model
    generalizes across a growing pool instead of one-hotting the card set);
  - `power`, `toughness` (post-layer computed), `mana_value`, `damage_marked`;
  - `status` bits (`tapped`, `flipped`, `face_down`, `phased_out`), `summoning_sick`;
  - controller (me/opp), `card_types`/`subtypes`/`colors` as small multi-hot, `keywords` as a
    keyword bitmask, counter summary from `CounterBag`, `attachments` count;
  - **combat role** (attacking / blocking / blocked-by-N) derived from `view.combat` (`CombatView`).
  `ObjView::Hidden` rows set a "hidden" flag and zero the rest. A parallel `entity_mask` marks
  occupied rows.
- **Own hand** (`Box[MAX_HAND, F]`): `me.hand` rows — `grp_id` embedding + a **castable** flag
  (whether a `Priority` request would currently list it; cheap to compute alongside `legal_actions`).
  Opponent hand is **count only** (already enforced — it isn't in the view).
- **Stack** (`Box[MAX_STACK, F]`): `view.stack` (`StackObjView`) — `grp_id`, controller (me/opp),
  source id, target refs (resolved to entity-row indices where possible).
- **Decision context**: a one-hot of *which* `DecisionRequest` variant this step is (21-way), plus a
  few request scalars (e.g. `ChooseNumber{min,max}`, `SelectCards{min,max}`) so the policy can route
  to the right head and respect bounds.
- **Action mask** (`MultiBinary[A]`): the legal-action mask for the current decision (§4). Always in
  `info["action_mask"]` (and optionally in the obs) so the policy can mask.

`MAX_PERM/MAX_HAND/MAX_STACK` are config; start small for the tiny pool and grow. The card-embedding
table is keyed by `grp_id` so a never-before-seen printing just needs a new row, not a reshape.

---

## 4. Action space + masking

The engine is the source of truth: it enumerates **only legal options** at each decision
(`legal_actions()` / the per-variant option vectors). The challenge is mapping 21 heterogeneous
request variants onto a **fixed, constant-width** action vocabulary `A` with a boolean mask, because
MaskablePPO needs a single `Discrete(A)` head + `MultiBinary(A)` mask.

### 4.1 The shape insight (reuse `options.rs`)

`crates/mtg-gre-server/src/options.rs` already collapses all 21 variants into **five answer shapes**
(`Mode`): `Action` (priority pick/pass), `SelectOne` (one index), `SelectMany` (a subset, `min..max`),
`Number` (an integer in a range), `Order` (a permutation). Every `DecisionResponse` the engine
accepts is built from these by `options::response_from`. The RL action codec is the **fixed-width
analog of that same projection** — so the env's encoder/decoder is a port of `options.rs` to a
tensor vocabulary, not new rules logic.

### 4.2 (A) Factored fixed vocabulary + legal mask (start here, MaskablePPO-friendly)

Partition `A` by decision kind, with **slots = positional indices into the padded observation**, so
an action head points at an entity row the policy already saw:

| Bucket | Encodes | Maps to `DecisionResponse` |
|---|---|---|
| `PASS` (1) | pass priority | `Pass` |
| `PLAY[hand_slot]` / `CAST[hand_slot]` | a `Priority` action | `Action(i)` |
| `ACTIVATE[perm_slot, ability_slot]` | a `Priority` activation | `Action(i)` |
| `TARGET[cand_slot]` | one target candidate | `Pairs((slot,cand))` (autoregressive over slots) |
| `ATTACK[perm_slot]` / `BLOCK[blk_slot,atk_slot]` | combat | `Pairs(...)` (autoregressive) |
| `SELECT[obj_slot]` | one card from a set | `Indices` (autoregressive subset) |
| `MODE[mode_slot]` / `OPTION[opt_slot]` / `COLOR[c]` | labeled picks | `Indices` |
| `NUMBER[bucket]` | bucketed integer | `Number(n)` |
| `ORDER[item_slot]` | next item in an ordering | `Order` (autoregressive permutation) |
| `CONFIRM[yes/no]` | binary | `Bool` |

At each step the env builds `mask[A]` from the current request's enumerated options: `mask[a] = 1`
iff global action `a` corresponds to a currently-legal option. The policy outputs logits over `A`,
sets illegal logits to `-inf`, softmaxes, samples. **It is structurally impossible to choose an
illegal action** (law #2) — the engine also defensively clamps any out-of-range response
(`priority.rs::distinct_valid_indices`, `parse_targets`), so a buggy policy can never wedge a game.

**Multi-select / structured decisions (`Indices`, `Pairs`, `Amounts`, `Order`, `Arrangement`,
`Payment`) decompose into autoregressive single-index sub-steps**: the env issues one flat
`Discrete(A)` action per sub-step (e.g. "pick the next attacker, or commit"; "pick this card's
target, then the next card's") and only assembles the full `DecisionResponse` once the seat commits.
This keeps the action space flat and the PPO math simple. **This decomposition is entirely
env-side** — the engine request stays *batched* (`DeclareAttackers` with all attackers at once),
preserving the 1:1 GRE alignment that keeps the web/MTGA adapter a pure translation (AGENT_INTERFACE
§3.1). `Distribute`/`AssignCombatDamage` (`Amounts`) start with the engine's auto-spread default
(what `RandomAgent`/`options.rs` already do) and gain a real head only when a card makes the split
matter.

### 4.3 (B) Autoregressive / pointer-network head (scale to here)

A single head that, conditioned on the decision-kind one-hot, emits a variable-length sequence of
pointers into the entity set (pick attacker → its defender → next…). Better for combinatorial
combat/targeting in one logical step and the natural fit for an AlphaZero-style policy. More complex
to train; adopt once (A) plateaus or the pool grows enough that flat autoregression is the
bottleneck. Either way **the policy only ever selects among engine-provided legal options.**

---

## 5. Reward, episode, and the priority-heavy nature

### Reward
- **Primary: sparse terminal** — `+1` win / `−1` loss / `0` draw, discounted. This is what we
  optimize; it avoids reward-hacking. Read it from `Engine::outcome()` /
  `GameState.{winner,end_reason}` (`ZeroLife`/`Decked`/`Poison`/`DrawOrCapped`).
- **Optional shaping (early milestones, annealed to 0):** small **potential-based** terms
  `F = γΦ(s') − Φ(s)` (Δlife differential, board presence, card advantage) so shaping is
  policy-invariant. Caution from `../magician`: a life-only linear baseline was weak
  (R²≈0.11 at turn 5) — shaping is a learning crutch, never the objective.

### Episode
- Episode = **one game**. `terminated` when `GameState.game_over` (life ≤0 / SBA loss / deck-out /
  poison ≥10); `truncated` on a turn/step cap (the engine already has `MAX_TURNS = 2000` as a
  draw-out backstop; the env sets a tighter training cap for mirror stalls).
- **Most steps are "pass priority"** — the dominant lever on episode length and throughput. Two
  mitigations, **both already implemented in the engine**:
  1. **Arena-profile auto-pass / stops** (`priority.rs`: `set_arena_auto_pass`, `StopConfig`,
     `should_auto_pass`) — the engine elides trivial priority windows uniformly for *every* backend
     (a seat with no meaningful action/stop is auto-passed and the `Agent` is never consulted). This
     is decided by the engine/Arena profile, **not** per-agent (AGENT_INTERFACE §8.1), so RL and the
     web client see the *same* decision points → replay/differential traces compare like-for-like.
     The gym enables this profile to cut steps/game dramatically.
  2. The `Priority` request always includes `Pass`, so when a window *is* surfaced, skipping is a
     single masked action.

> Determinism caveat for RL: with auto-pass **off**, every window prompts (paper-CR, fully
> deterministic — what differential/replay tests use). With it **on**, fewer prompts but still
> deterministic for a fixed profile+seed. Pick one profile per training run and pin it.

### `MtgEnv` skeleton (Gymnasium API)

```python
class MtgEnv(gym.Env):                 # wraps PyGame from crates/mtg-py
    def reset(self, seed=None, options=None) -> tuple[obs, info]      # info["action_mask"]
    def step(self, action) -> tuple[obs, reward, terminated, truncated, info]
    # observation_space: gym.spaces.Dict(...)   (§3)
    # action_space:      gym.spaces.Discrete(A)  (+ MultiBinary(A) mask in info)  (§4)
```
Self-play: the env holds **both seats**; the learning policy answers the active seat's `decide`, the
opponent (frozen checkpoint / random / scripted) answers the other — or expose both seats and let
the trainer route. `info["action_mask"]` is provided every step (SB3-contrib `MaskablePPO` consumes
it directly).

---

## 6. Throughput, vectorization, self-play

> **M2 measurement (2026-06-13, demo deck) — corrects the assumption below.** Once a *NN opponent*
> is in the loop (self-play), the simulator is **not** the bottleneck — inference is. Measured:
> raw engine **54 games/s/core** (no NN); self-play `DummyVecEnv(8)` **~14 games/s/core** (capped by
> the per-env *synchronous opponent* `predict`); **`SubprocVecEnv` was *slower*** (~0.6/core) because
> the large `Dict` observation makes per-step pipe IPC cost more than the parallelism buys on a fast
> sim — so multiprocessing is the wrong lever here. Hitting the ≥10² games/s/core bar **with the NN**
> needs **async batched inference** (one forward over all envs' pending decisions, learner *and*
> opponent), which is cleanest after M3's resumable step API removes the per-game threads. Tracked as
> a deferred item (paired with M3); M2 ships on `DummyVecEnv` (self-play trains + improves in minutes
> at demo scale). The bar was always for M4-scale training.

- **Why Rust matters:** the simulator, not the (small, GPU-batched) net, is the self-play
  bottleneck. A pure-Python engine caps at ~10²–10³ decisions/s; Rust + no-serialization PyO3 targets
  ~10⁵–10⁶ simple decisions/s/thread, i.e. ~10²–10³ *games*/s/core on the tiny pool, scaling with
  cores. Auto-pass (§5) is the dominant multiplier on effective episode length. (But see the M2 note
  above: the *NN inference*, not the sim, gates self-play throughput until async batched inference.)
- **Vectorized envs:** `N` independent `PyGame`s (each its own game thread under approach 2.2-A, or
  a cheap re-entrant handle under 2.2-B). Games desync (different decision kinds per env), so the
  throughput sweet spot is **async batched inference**: each env advances to its next decision, the
  collector gathers a batch of pending `(obs, mask)`, runs one GPU forward, and scatters actions
  back. Group/pad by decision-kind for a clean batch.
- **Multiprocessing:** several such processes across cores (Gym `AsyncVectorEnv` or a custom
  shared-memory collector) to saturate CPU; GPU does inference centrally.
- **Snapshotting:** `GameState` is `Clone` + serde (card data shared behind `Arc<CardDb>`,
  `#[serde(skip)]`; RNG a seeded `u64`), so `PyGame.snapshot()/restore()` and `PyGame.clone()` give
  (a) **MCTS/AlphaZero** rollouts from a node, (b) exact-replay differential/regression testing,
  (c) cheap `reset` by restoring a pre-rolled opening. Clones replay deterministically. (ENGINE_PLAN
  §7 notes a future per-clone shrink — read printed chars from `CardDb` instead of duplicating
  `Characteristics` on every `Object` — neither foreclosed nor required for milestone 0.)
- **Determinism/seeding:** one seed per game seeds `GameState.rng` (shuffles, any coin flips). Same
  seed + same policy (greedy) ⇒ identical game ⇒ replayable for debugging and for verifying a
  checkpoint.

---

## 7. Testing / validation (CR + captured MTGA logs, not an external engine)

The engine's correctness is validated against the **paper Comprehensive Rules** (CR-derived
`expect-test` snapshots co-located in `mtg-core`) and the **captured MTGA Detailed-Logs GRE stream**
(`../mtga-re`) — *not* by differential-testing against another engine. For the gym specifically:

- **Boundary/throughput smoke (milestone 0):** two `RandomAgent`-equivalent policies (or the
  in-Rust `RandomAgent` on one seat, `PyAgent` on the other) play thousands of legal games to
  termination with no rules panics and a **non-empty action mask at every decision**. Assert card /
  life / zone conservation invariants (the engine's `priority.rs` tests already do this for
  lands-only and combat games — reuse the harness).
- **Determinism replay (`ScriptedAgent` + golden corpus):** record a game's seed + decision log,
  replay through a fresh `Engine`, assert byte-identical `GameState` snapshots turn-by-turn. This is
  the env's reproducibility guarantee and doubles as a regression corpus as the pool grows. (The
  `ScriptedAgent` is a trivial `Agent` that replays a fixed `DecisionResponse` list.)
- **Encoder/codec round-trip:** for every `DecisionRequest` variant, assert the obs encoder produces
  a finite tensor and the action codec ∘ mask only ever yields an in-range `DecisionResponse`
  (`expect-test` over representative requests — mirror `agent.rs::wire_snapshots` and
  `options.rs::tests`).
- **Learning sanity (milestone 1):** MaskablePPO win-rate vs a random opponent climbs above 50% on
  the tiny pool (the real signal that obs + mask + reward are wired correctly).

---

## 8. Milestones

Ordered; each builds on the last and is independently testable. The boundary (L2) and engine (L1)
already exist — these are the L3/L4/L5 build.

0. **PyO3 boundary + random self-play, tiny pool.** Stand up `crates/mtg-py` (maturin) with the
   `PyGame` handle and the thread+channel `PyAgent` (§2.2-A, port of `GreSessionAgent`). Minimal
   `MtgEnv`. Two random policies play legal games to termination. **Exit:** thousands of legal
   games/s, no panics, non-empty mask at every decision, conservation invariants hold.
1. **Observation encoder + factored action space + PPO smoke.** Rust→numpy obs encoder (§3); the
   factored action vocabulary + masking (§4-A); reward = sparse terminal (+ annealed shaping). Train
   `MaskablePPO` on the tiny pool vs a random opponent. **Exit:** win-rate beats random; codec
   round-trip tests green.
2. **Self-play league + snapshotting + vectorization.** Frozen-opponent pool / self-play;
   `snapshot/restore`/`clone`; vectorized envs + async batched inference; enable the Arena auto-pass
   profile. Grows naturally as the engine adds mechanics (removal, modal spells, triggers, mulligan
   — already partly present). **Exit:** stable self-play improvement; ≥10² games/s/core.
3. **Resumable step API (engine, coordinated) — throughput only.** Land approach §2.2-B with
   `engine` (re-entrant `resume`/`submit`), swap `PyAgent` internals to it (no Python API change).
   RE-SCOPED 2026-07-03 (user directive): engine-backed tree search is permanently out of scope
   (MuZero-style *learned* search needs no engine support), so clone/snapshot-for-search is CUT as
   a deliverable — `PyGame.snapshot/restore/clone` stay stubbed. Design: RESUMABLE_ENGINE.md
   (stackful-coroutine session; fiber is deliberately not cloneable). **Exit:** per-game
   threads/channels removed; GIL-free Rust fleet stepping with one PyO3 crossing per micro-tick;
   measured training-throughput win vs the n_envs-scaling baseline.
4. **Scale the card pool.** The `grp_id` embedding table grows; load a larger pool / a real limited
   format (ties to `../magician` 17lands card priors). Consider the pointer-network action head
   (§4-B). **Exit:** competent agent on a real format vs the Rust scripted baseline and prior
   self-play checkpoints.
5. **(Stretch) shared infra with the GRE client.** Nothing new in the gym — but a policy trained
   here is, by construction, a drop-in `Agent` the `mtg-gre-server` (and later `MtgaClientAgent`,
   `docs/plans/CLIENT_PLAN.md`) can run, because all three speak the *same* boundary.

---

## Appendix — concrete first changes (when the gym workstream starts)

- Add workspace crate **`crates/mtg-py`** (PyO3 `cdylib`, maturin) depending only on `mtg-core`;
  wrap the `Agent` boundary with the thread+channel `PyAgent` (lift the pattern from
  `crates/mtg-gre-server/src/session.rs`). No engine logic in this crate.
- New **`python/mtgenv_gym/`**: `MtgEnv`, the obs-encoder glue + action codec (port the shape logic
  of `crates/mtg-gre-server/src/options.rs` to a fixed-width vocabulary), a `MaskablePPO` entrypoint,
  and a vectorized/async rollout collector.
- Reuse `mtg-core`'s seeded `Rng` + serde snapshotting for reproducible rollouts / MCTS; reuse the
  `priority.rs` conservation-invariant test harness for the milestone-0 smoke.
- **Coordinate with `engine`** on the resumable `resume`/`submit` step API (§2.2-B) — spec agreed
  here, owned and built by `engine`; the gym ships on approach (A) until it lands.
- Validate engine rules via the existing CR-derived `expect-test` suite + captured MTGA logs — the
  gym adds *its own* encoder/codec/determinism tests, not a cross-engine oracle.
