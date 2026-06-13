# GYM_PLAN — Python RL Training Environment for the mtgenv Rust MTG Engine

Status: design plan (no code yet). Companion: [`DECOMPILE_PLAN.md`](./DECOMPILE_PLAN.md) (MTGA GRE protocol decompile, written in parallel — referenced below for the shared decision interface).

Goal: a high-throughput Gymnasium environment that wraps the from-scratch Rust MTG rules engine in `/home/xander/dev/p-mtg/mtgenv`, to train an MTG agent in Python + PyTorch via self-play. The decision interface must be backend-agnostic so the *same* abstraction serves three drivers interchangeably: (1) a native Rust heuristic AI, (2) a Python RL policy, (3) the future decompiled MTGA GRE client.

This plan deliberately mirrors two existing systems the user has built/worked with:
- **Forge** (`/home/xander/dev/p-mtg/forge-ai`): a complete Java MTG engine whose `PlayerController` (107 abstract decision methods) is reduced to **8 RL decision types** (`PlayerControllerRl.RlDecisionType`) and bridged to Python over a line-delimited JSON TCP socket on port 12345 (`RlBridge.java` ↔ `python/forge_rl_gym.py:ForgeEnv`). We adopt its decision-abstraction granularity but reject its transport for the hot path.
- **magician** (`/home/xander/dev/p-mtg/magician`): 17lands-style per-turn columnar features (life, hand/board counts, mana spent, cast/attack/block events, per-card pool/deck vectors) — informs the observation encoding.

Lessons from the abandoned `from-scratch/mtgai`: do **not** wrap an existing engine's internals (it got stuck on Forge XStream deserialization and tight coupling). The clean-room Rust engine avoids this; we own the decision boundary end to end.

---

## 1. Architecture

Five layers, each with a single responsibility and a stable interface to the next:

```
┌──────────────────────────────────────────────────────────────────────────┐
│ L5  PyTorch training (self-play, PPO / AlphaZero-style)                     │
│     policy/value nets, replay buffer, league/opponent pool                  │
└───────────────▲───────────────────────────────────────────┬───────────────┘
                │ batched obs tensors + action-masks          │ actions
┌───────────────┴───────────────────────────────────────────▼───────────────┐
│ L4  Gymnasium env  (mtgenv_gym, Python)                                     │
│     MtgEnv(gym.Env): reset/step, obs encoder, action (de)coder, reward      │
│     VecEnv wrapper over N in-process Rust games                             │
└───────────────▲───────────────────────────────────────────┬───────────────┘
                │ DecisionRequest (struct→numpy)              │ DecisionResponse (idx)
┌───────────────┴───────────────────────────────────────────▼───────────────┐
│ L3  PyO3 bindings  (mtgenv-py crate, maturin)                               │
│     PyGame: new/reset/step_until_decision/apply_decision/snapshot/clone     │
│     zero-copy obs via numpy; request carries legal-action mask              │
└───────────────▲───────────────────────────────────────────┬───────────────┘
                │ Agent::decide(req) -> resp                  │
┌───────────────┴───────────────────────────────────────────▼───────────────┐
│ L2  Decision / Agent interface  (Rust trait `Agent`)                        │
│     engine pauses at a decision point, builds a typed DecisionRequest with  │
│     ALL legal options enumerated, asks the active player's Agent, applies   │
│     the returned DecisionResponse. Backends: RustHeuristicAgent,            │
│     PyAgent (PyO3 callback / channel), SocketAgent (MTGA / Forge-style).    │
└───────────────▲───────────────────────────────────────────┬───────────────┘
                │ decision request                            │ chosen option
┌───────────────┴───────────────────────────────────────────▼───────────────┐
│ L1  Rules engine core  (headless, deterministic, no I/O)                    │
│     GameState, zones, turn/phase loop, priority, stack, SBAs, combat,       │
│     triggers, targeting, mana. Pure fn of (state, decision) -> state.       │
└────────────────────────────────────────────────────────────────────────────┘
```

### Decision request/response flow (the core control loop)

The engine is a **driver** that runs until it needs a choice from a player, then yields. There is no callback into Python in the inner loop's default form — the env *pulls* requests (matches Forge's `ForgeEnv.step` pulling a `decision_request`, and matches how a Gym `step` consumes one action).

```
                 L1 engine                         L4 Gym env / L5 policy
   run_until_decision()
        │ advance phases, put triggers on stack,
        │ check SBAs, grant priority ...
        │ reach a point requiring a choice
        ▼
   build DecisionRequest {
       kind: ChooseTargets|DeclareAttackers|... ,
       actor: PlayerId,
       options: [legal opt 0..K],     ◄── engine enumerates ONLY legal options
       mask:    [1,1,0,1,...],        ◄── derived; redundant w/ options but flat
       min,max, context(state digest)
   }
        │  (yield to boundary L3)
        ▼  PyGame.step_until_decision() returns (obs, request)
                                          │
                                  encode obs tensor + action mask
                                          │  policy.forward(obs, mask)
                                          ▼  sample legal action -> option index i
                                  PyGame.apply_decision(i)
        ┌─────────────────────────────────┘
        ▼
   apply DecisionResponse{ chosen: i }
        │  mutate GameState, continue run_until_decision()
        ▼  ... until terminal (a player loses) -> reward
```

Key property: **the engine never asks an open-ended question.** Every decision point ships the *complete enumerated legal set*. The policy only ever selects an index (or a subset of indices) into that set. This is the single most important design decision (see §3 action masking).

Engine core is **pure and deterministic**: `apply(state, decision, rng) -> state`. RNG is an explicit seeded field of `GameState` so games are reproducible and snapshot/clone is trivial (enables MCTS rollouts and exact replay for differential testing).

Current code (`src/game.rs`) already has the right bones: a `Decision` enum, a `DecisionPoint` trait, `GameError`, `next_phase()`. We will refactor `DecisionPoint` (generic-associated, hard to make into a trait object) into the unified `Agent` + `DecisionRequest`/`DecisionResponse` model below, and grow the `Decision` enum into the tagged request enum.

---

## 2. Rust ↔ Python boundary

**Recommendation: PyO3 + maturin, in-process, for the RL env hot path. Keep a `SocketAgent` transport as a first-class alternative behind the same `Agent` trait for the MTGA/Forge backends.**

### Options compared

| Transport | Latency / call | Serialization | Languages | Snapshot/clone | Fit |
|---|---|---|---|---|---|
| **PyO3 in-process** (recommend) | ~tens of ns–µs (FFI call) | none for state; zero-copy numpy for obs | Rust↔Python only | trivial (clone Rust struct) | RL self-play hot path |
| Socket + JSON (Forge `RlBridge`) | ~50–500 µs + parse | full JSON per decision | any | hard (state lives in engine proc) | MTGA client, Forge interim, cross-host |
| cffi / raw C FFI | low | manual C structs, unsafe | any w/ C ABI | manual | not worth it vs PyO3 |
| Shared-memory ring + flatbuffers | low | schema, zero-copy | any | medium | overkill now; revisit for distributed |

### Why PyO3 wins the inner loop

A single MTG game is **decision-heavy**: most decisions are "pass priority." At ≥10³–10⁴ decisions/game and target ≥10³ games/sec, a socket round-trip per decision (Forge's model: JSON encode → TCP → parse → respond → parse) is the bottleneck — Forge's bridge is fine for human-speed play and for a *single* learning game, but collapses under vectorized self-play. PyO3 keeps `GameState` in the same address space as the Python process: a decision is a plain function call, and observations are written into a preallocated numpy buffer (`PyArray`, zero-copy). No serialization on the hot path.

Forge's bridge also has fragile hand-rolled JSON parsing (`RlBridge.parseJsonResponse` substring-scans for `"chosenIndices":[`), a stub 1000-float observation, and a `Box(10,)` continuous action space that ignores legality — we improve on all three.

### Why we still design for sockets

The decompiled **MTGA GRE** speaks a wire protocol (see `DECOMPILE_PLAN.md`): the engine-side `SocketAgent` must translate GRE prompts ↔ our `DecisionRequest`. Forge could also serve as an interim Gym backend through its existing port-12345 bridge. So the `Agent` trait (L2) is transport-agnostic; PyO3's `PyAgent` and a `SocketAgent` are two implementations. The boundary is the **trait**, not the transport.

### Concretely
- New crate `mtgenv-py` (PyO3 `cdylib`), built with **maturin**; per the user's bin/lib rule it is a thin binding crate depending on the `mtgenv` engine lib — no engine logic in it.
- Add to engine `Cargo.toml`: keep `serde`/`serde_json` (used by `SocketAgent` and snapshots), drop `egui`/`eframe` from the core lib (move any GUI to a separate optional bin target — it bloats the lib and is irrelevant to headless training).
- The PyO3 surface is intentionally tiny: `PyGame::{new(config), reset(seed), step_until_decision() -> (obs, PyRequest), apply_decision(idx_or_indices), legal_mask(), snapshot()->bytes, restore(bytes), clone()}`. Observation encoding can live in Rust (fast, write into numpy) — preferred — or in Python; start in Rust for speed.

---

## 3. The agent / decision interface (the crux)

A single Rust trait. The engine, when it needs a choice, constructs a typed `DecisionRequest` whose every variant carries the **fully enumerated legal options**, hands it to the active player's `Agent`, and consumes a `DecisionResponse` (indices into those options). This is the join point that makes Rust-AI, Python-RL, and MTGA-client interchangeable.

```rust
/// One per seat. Engine calls `decide` whenever the seat must choose.
pub trait Agent {
    fn decide(&mut self, req: &DecisionRequest, view: &PlayerView) -> DecisionResponse;
    /// Push-only notifications (reveals, results) — no response. Mirrors Forge
    /// PlayerController.reveal()/notifyOfValue(); GRE "GameStateMessage" deltas.
    fn observe(&mut self, _ev: &GameEvent, _view: &PlayerView) {}
}

/// Tagged enum of every decision the engine can ask. Each variant pre-enumerates
/// legal options so the agent only ever returns indices. Mirrors Forge's 8
/// RlDecisionType buckets AND the MTGA GRE prompt families.
pub enum DecisionRequest {
    /// Priority: cast spell / activate ability / play land / special action / pass.
    /// Replaces Forge chooseSpellAbilityToPlay + the implicit "pass" most steps are.
    Priority      { actions: Vec<PlayableAction>, can_pass: bool },        // PLAY_SPELL_OR_ABILITY
    ChooseTargets { for_action: ActionRef, slots: Vec<TargetSlot> },       // CHOOSE_TARGETS
    PayCost       { cost: CostRequest, sources: Vec<ManaSource> },         // (mana/sacrifice/discard costs)
    DeclareAttackers { eligible: Vec<AttackerOption> },                    // DECLARE_ATTACKERS
    DeclareBlockers  { eligible: Vec<BlockerOption>, attackers: Vec<PermanentId> }, // DECLARE_BLOCKERS
    OrderObjects  { what: OrderKind, items: Vec<ObjId> },   // trigger order / blocker order / damage order
    AssignDamage  { attacker: PermanentId, recipients: Vec<DamageSlot>, total: u32 },
    Mulligan      { hand_digest: HandDigest, london_to_bottom: u32 },      // MULLIGAN_DECISION
    ChooseMode    { modes: Vec<ModeOption>, min: u8, max: u8 },            // modal spells
    ChooseCards   { zone: ZoneRef, options: Vec<ObjId>, min: u8, max: u8 },// CHOOSE_CARDS_FROM_LIST (sac/discard/search/scry/surveil)
    ChooseNumber  { min: i32, max: i32 },                                  // CHOOSE_NUMBER (X, divide, etc.)
    ChooseColor   { allowed: Vec<Color> },                                 // part of CHOOSE_OPTION_FROM_LIST
    ChooseOption  { options: Vec<OptionLabel> },                           // CHOOSE_OPTION_FROM_LIST (type/vote/sprocket)
    Confirm       { prompt: ConfirmKind },                                 // CHOOSE_BINARY (yes/no, optional triggers)
}

/// Indices into the request's option vectors (+ payloads for damage split etc.).
pub enum DecisionResponse {
    Index(u32),
    Indices(Vec<u32>),                 // multi-select (modes, cards, attackers)
    Pairs(Vec<(u32, u32)>),            // blocker->attacker, damage->recipient
    Number(i32),
    Pass,
}
```

### Why a tagged enum with enumerated options (not Forge's 107 methods, not GRE's open prompts)

- **Forge**: 107 `PlayerController` methods (`/home/xander/dev/p-mtg/forge-ai/forge-game/src/main/java/forge/game/player/PlayerController.java`) collapsed to 8 `RlDecisionType`s. We use ~14 variants — slightly finer than 8 (we split priority/targets/cost/order/damage rather than lumping into `CHOOSE_*`) because finer typing gives the policy a cleaner per-head structure (§4) and matches GRE prompt families more directly, while staying a closed set the engine can exhaustively produce.
- **MTGA GRE**: prompts are `Select N from M`, `DeclareAttackers`, `DeclareBlockers`, `PayCost`, `OrderTriggers`, `ChooseTargets`, etc. — our enum is a superset; `SocketAgent` maps GRE `PromptMessage` → `DecisionRequest` and our `DecisionResponse` → GRE actions. Alignment here is what lets a trained policy later drive the real client.

### Action masking — the engine is the source of truth

The engine enumerates **only legal options** at every decision point (legality = rules + timing + targeting restrictions + mana availability). The mask is therefore `[1]*len(options)` for that step, but we expose it explicitly against a **fixed global action vocabulary** (§4) so the Python side gets a constant-width boolean mask: `mask[a] = 1` iff global action `a` corresponds to a currently-legal option. The policy multiplies logits by the mask (set illegal to `-inf` before softmax). This is the fix for Forge's `Box(10,)` action space that ignored legality and just did `int(action[0]) % len(options)`.

### Backends implementing `Agent`
- `RustHeuristicAgent` — rule-based baseline & opponent (port the spirit of Forge `ComputerUtil*`); also the random agent for milestone 0.
- `PyAgent` — for PyO3: when the engine reaches `decide`, it does **not** call into Python; instead `step_until_decision` returns the request to Python, the policy picks, `apply_decision` resumes. (Implemented as a one-shot channel / coroutine yield so the env's `step` semantics line up; avoids re-entrant GIL calls.)
- `SocketAgent` — line-delimited JSON or GRE binary; serves Forge-interim backend and the MTGA client. Same trait, different transport.

---

## 4. Gymnasium env design

### Observation space

A `Dict`/structured space encoded as fixed tensors, drawing on magician's feature taxonomy (`replay_dtypes.py`: per-turn life, hand/board counts, mana spent, cast/attack/block events, per-card pool/deck vectors) but made *current-state* rather than per-turn-historical:

- **Global scalars** (`Box`): turn #, phase one-hot (12, from `Phase` in `types.rs`), active/priority player, both life totals, poison, hand/library/graveyard/exile sizes per player, lands-in-play, available mana by color, stack depth. (magician's `eot_*` columns are exactly these.)
- **Card/permanent set** (`Box[max_objs, feat]`): per object on battlefield/stack/relevant zones — a learned/embedded card id, P/T, damage, tapped, summoning-sick, counters, controller, attacking/blocking, keyword bitmask. Pad to `MAX_OBJECTS`; provide an entity-mask. This is the structured analog of magician's `pool_*`/`deck_*` per-card columns, but per *instance*.
- **Hand** (`Box[max_hand, feat]`): card-id embedding + castability flags for own hand (opponent hand as count only — hidden info).
- **Stack** (`Box[max_stack, feat]`): object id, controller, target refs.
- **Action mask** (`MultiBinary[A]`): the legal-action mask for the *current* decision (see action space). Always part of the observation so the policy can mask.
- **Decision-kind one-hot**: which `DecisionRequest` variant this step is — lets the policy route to the right head.

Card identity uses an **embedding table keyed by oracle/card id** (not one-hot over the set), so the model generalizes across a growing card pool — start tiny (milestone 0) and grow. Hidden information (opponent hand/library order) is never leaked into the observation.

### Action space (the hard part)

MTG's action space is huge and variable. Two viable designs; we recommend (A) to start, with a clear path to (B):

- **(A) Factored fixed action space + legal mask (start here).** Define a fixed global action vocabulary `A` partitioned by decision kind:
  - `PASS` (1),
  - `PLAY[hand_slot]` / `ACTIVATE[perm_slot, ability_slot]`,
  - `TARGET[slot, object_slot]`,
  - `ATTACK[perm_slot]`, `BLOCK[blocker_slot, attacker_slot]`,
  - `CHOOSE[option_slot]`, `NUMBER[bucketed]`, `CONFIRM[yes/no]`, `MODE[mode_slot]`, `ORDER[item_slot]`.
  Slots are positional indices into the padded observation (hand_slot ↔ hand tensor row, etc.). At each step the env builds `mask[A]` from the request's enumerated options; the policy outputs logits over `A`, masks, samples. Multi-select decisions (declare attackers, choose N cards) are handled by **autoregressive sub-steps**: the engine re-asks ("add another attacker or commit") so each env `step` is a single index — keeps the action space flat and the math simple. This is the pragmatic, PPO-friendly choice and directly cures Forge's continuous-`Box(10,)`-modulo hack.
- **(B) Autoregressive / pointer-network action head (scale to here).** A single head that, conditioned on decision kind, emits a variable-length sequence of pointers into the entity set (e.g., pick attacker, pick its target, ...). Better for combinatorial decisions (full attacker/blocker assignment in one step) and is the natural fit for an AlphaZero-style policy. More complex to train; adopt once (A) plateaus.

Either way: **the policy only ever selects among engine-provided legal options.** The action space is a fixed vocabulary; the mask makes it valid.

### Reward

- **Primary: sparse terminal** `+1` win / `−1` loss (`0` draw), discounted. This is what we ultimately optimize and avoids reward-hacking.
- **Optional shaping (early milestones, annealed to 0):** small potential-based terms (Δlife differential, board presence, card advantage) to bootstrap learning on the tiny pool. Use potential-based shaping `F = γΦ(s') − Φ(s)` so it's policy-invariant. magician's linear life-only baseline (R²≈0.11 at turn 5) is a caution: life alone is weak — use shaping only as a learning crutch, not the objective.

### Episode & the priority-heavy nature

- Episode = one game; `terminated` when a player loses (life ≤0, SBA, deck-out, poison ≥10), `truncated` on a turn/step cap (mirror-stall protection).
- **Most steps are "pass priority."** Two mitigations: (1) the engine **auto-passes** trivial priority windows where the player has no legal non-pass action and no instant-speed options it might want (configurable "stops" like MTGA), so the policy is only consulted at meaningful decision points — drastically cuts steps/game; (2) the `Priority` request always includes `PASS`, and skipping is one masked action otherwise. This is the single biggest lever on effective episode length and throughput.

### `MtgEnv` skeleton (Gymnasium API)

```python
class MtgEnv(gym.Env):                # wraps PyGame from mtgenv-py
    def reset(self, seed=None, options=None) -> (obs, info)      # info["action_mask"]
    def step(self, action) -> (obs, reward, terminated, truncated, info)
    # observation_space: gym.spaces.Dict(...)  (above)
    # action_space:      gym.spaces.Discrete(A)  (+ MultiBinary(A) mask in obs/info)
```
Self-play: the env holds **both** seats; the learning policy plays the active seat each `decide`, the opponent (frozen policy / random / heuristic) plays the other — or expose both seats and let the trainer route. Provide `info["action_mask"]` every step (SB3-contrib `MaskablePPO` consumes it).

---

## 5. Throughput & self-play

- **Why Rust matters:** the sim is the bottleneck in self-play, not the net (small nets, batched on GPU). A pure-Python engine would cap at ~10²–10³ decisions/s; Rust + no-serialization PyO3 targets ~10⁵–10⁶ simple decisions/s single-thread, i.e. ~10²–10³ *games*/s/core on a tiny pool, scaling with cores.
- **Vectorized envs:** `N` independent `PyGame` instances in one process (Rust is `Send`); step them in lockstep and **batch observations into one tensor for a single GPU forward**. Because games desync (different decision kinds per env), group by decision-kind or pad+mask; or use an async actor model (each env advances to its next decision, collect a batch of pending requests, one inference, scatter actions back). The async-batched-inference pattern is the throughput sweet spot.
- **Multiprocessing:** run several such processes across cores (Gym `AsyncVectorEnv` / a custom shared-memory collector) to saturate CPU; GPU does inference centrally.
- **Snapshotting:** `GameState` is `Clone` + serde — `PyGame.snapshot()/restore()` enables (a) MCTS/AlphaZero rollouts from a node, (b) exact-replay differential testing (§6), (c) cheap env reset by restoring a pre-rolled opening. Seeded RNG in `GameState` makes clones deterministic.
- **Rough target:** milestone-0 tiny pool ≥10³ games/s on a workstation (multi-core); realistic constructed games (deeper stacks, more triggers) ≥10²/s/core. Revisit with profiling; auto-pass (§4) is the dominant factor.

---

## 6. Differential testing / interim backend (use Forge)

Forge is a battle-tested, near-complete MTG engine — exploit it twice:

- **(a) Oracle for rules correctness.** For a shared card subset, drive **both** Forge and mtgenv through an **identical scripted decision sequence** (seeded) and assert identical resulting game states (zones, life, stack, counters, SBA outcomes). Mechanism: a `ScriptedAgent` on the mtgenv side and a `PlayerController` that replays the same script on Forge (Forge already supports headless sim: `run-forge-headless.sh sim ...`). Diff state digests turn-by-turn; mismatches localize rules bugs. Forge's `PlayerController` method set is the **checklist of decision types** mtgenv must eventually cover. Start with a golden-game corpus (random-agent games on the tiny pool) replayed on both.
- **(b) Interim Gym backend while the Rust engine matures.** Forge's existing `RlBridge`/`ForgeEnv` (port 12345, line-delimited JSON, `PlayerControllerRl`'s 8 decision types) can back `MtgEnv` *today* via the same `Agent`-shaped interface (our `SocketAgent` semantics) — letting L4/L5 (env, encoders, training loop) be built and validated against real full games before L1 is complete. This de-risks the Python side: when mtgenv reaches parity, swap the transport from socket-to-Forge to in-process PyO3 with no change to the policy/observation/action code. (Forge's bridge needs the observation/reward TODOs filled in to be useful as more than a smoke test.)

---

## 7. Milestones

Ordered; each builds on the last and is independently testable.

0. **Boundary + random self-play, tiny pool.** Refactor `DecisionPoint` → `Agent` + `DecisionRequest`/`Response` (§3); implement engine core enough for vanilla creatures + lands + combat + priority on a ~10-card pool. Stand up `mtgenv-py` (PyO3/maturin) and a minimal `MtgEnv`. Two `RandomAgent`s play legal games to termination. **Exit:** thousands of legal random games/s, no rules panics, action mask always non-empty.
1. **Observation + action mask + PPO smoke.** Implement the structured observation encoder (Rust→numpy) and the factored action space (A) with masking. Train `MaskablePPO` on the tiny pool vs a random opponent; confirm win-rate climbs above 50%. **Exit:** learning curve beats random; reward = sparse terminal (+ annealed shaping).
2. **Self-play league + snapshotting.** Frozen-opponent pool / self-play; `snapshot/restore`; vectorized + batched inference. Add more mechanics (targeted removal, modal spells, triggers, mulligan). **Exit:** stable self-play improvement; ≥10² games/s/core.
3. **Differential testing vs Forge.** Build the `ScriptedAgent`/golden-corpus harness (§6a); reach state-parity with Forge on the shared pool; fix rules bugs surfaced. Optionally wire the Forge `SocketAgent` interim backend (§6b). **Exit:** zero state-digest diffs on the corpus.
4. **Scale the card pool / a real limited format.** Card-embedding table grows; load a real set (e.g., a draft/sealed pool — ties to magician's 17lands data for card priors). Train at scale; consider autoregressive action head (B) and/or AlphaZero-style MCTS using `snapshot`. **Exit:** competent agent on a real format vs the Rust heuristic baseline and prior self-play checkpoints.
5. **(Stretch) MTGA client target.** Per `DECOMPILE_PLAN.md`, implement `SocketAgent`↔GRE so a trained policy drives the real client through the *same* `DecisionRequest` interface.

---

## Appendix — concrete first changes to `/home/xander/dev/p-mtg/mtgenv`

- `src/game.rs`: replace `DecisionPoint` (GAT, not object-safe) with `Agent` trait + grow `Decision` into `DecisionRequest`/`DecisionResponse` (§3). Add seeded RNG field to `GameState`; derive `serde` for snapshot.
- `Cargo.toml`: move `egui`/`eframe` out of the core lib into an optional GUI bin target; keep `serde`/`serde_json`/`thiserror`. Add a new workspace member `mtgenv-py` (PyO3 `cdylib`, maturin) depending on the engine lib only.
- New `python/mtgenv_gym/`: `MtgEnv`, obs encoder glue, `MaskablePPO` training entrypoint, vec/async collector.
- New `tests/`: scripted-game differential harness vs Forge (`run-forge-headless.sh sim`).
