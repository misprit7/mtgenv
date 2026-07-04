# Stochastic MuZero on the swine environment

**Exploratory workstream.** Question: does learned MCTS lookahead (Stochastic MuZero) beat
the model-free PPO baseline on the swine matchup — *especially* on combat judgment, where PPO
has a known, stubborn failure (it chump-blocks the trampling 3/3 even at high life)?

Everything here is **isolated**: this dir has its own venv, README, and `.gitignore`. It only
*reads/imports* `python/mtgenv_gym/` and `crates/` — it never modifies them.

> Status: **M0 ✅ · M1 ✅ · M2 ✅ · M3 negative · DEBUG AUDIT (2026-07-04) ✅ — mechanism found.**
> The plumbing is CLEAN; the collapse is a real *policy-collapse-to-a-low-index attractor* under
> sparse reward + a sharp collection temperature, NOT a reward/perspective bug. See the audit
> section immediately below; the old M3 write-up (kept further down) misattributed the cause.

## DEBUG AUDIT (2026-07-04) — plumbing is clean; the collapse is an exploration failure

**Prompt:** the user suspected a *bug* (reward/perspective/terminal plumbing), because a random-vs-random
mirror wins ~50% so the buffer "should be full of +1", contradicting "value −0.8 everywhere".

**Finding — the plumbing is clean; the buffer really is ~all-losses; value −0.8 is faithful, not a bug.**
The killer-argument premise (early collection ≈ random ≈ 50%) is false: the MuZero *collector* collapses
to ~100% losses within ~2 collects, so a value net that predicts ≈ −0.8 everywhere is *correctly*
fitting an all-loss buffer.

Evidence (scripts in this dir; run with `PYTHONPATH=../../python .venv/bin/python <script>`):

| check | script | result |
|---|---|---|
| adapter reward/terminal/perspective | `audit_plumbing.py` | uniform-random win 0.510 swine / 0.467 heralds; terminal reward ∈{−1,0,+1}, ~½ positive; `eval_episode_return`==final reward (0/300 mismatch); **reward 0 on every non-terminal step**; len ~62 |
| mulligan phase, by hand | `audit_mulligan.py` | no spurious reward; masks sane (96=mulligan, 97=keep); reward 0 until terminal |
| value-target math | (read `lzero/mcts/buffer/game_buffer_muzero.py`) | `not_board_games` ⇒ **no sign flip** (L483-5, L510-1); `to_play=−1` single-agent; targets faithfully track the buffer |
| untrained MCTS, sampled vs greedy | `audit_untrained_mcts.py`, `audit_mcts_fullgame.py` | collect/**sampled** (temp 0.25) wins **0.525**; eval/**greedy** wins **0.000** |
| real collector, instrumented | `audit_collect_trace.py` | first collect wins ~0.39; degrades to ~0.05; games normal-length (median 29), **median 0 mulligans** |

**Mechanism — lowest-index tie-break → PASS/mulligan attractor, amplified by sparse reward + temp 0.25.**
Near-tied visit counts make argmax pick the **lowest legal index**, which is PASS(0) at every Priority
window, mulligan(96) at Mulligan, "no attack"(0) at DeclareAttackers → greedy play = do-nothing = 0% win.
Collection *samples* (temp 0.25) so it starts healthy (~40–52%), but over collects the policy head imitates
the slightly low-index-tilted visits, the visit distribution sharpens, temp-0.25 sampling becomes
effectively greedy, play goes passive, the buffer fills with losses, value→−1, and it self-reinforces.
There is **no exploration floor** (`random_collect_episode_num=0`, `eps_greedy_exploration_in_collect=None`).
The README's original "always-mulligan" is the *eval/greedy* symptom; *collection* loses via passive play in
normal-length games, not mulligan-to-death.

**Candidate fixes (being tested on the heralds falsifier):** raise the collection temperature (→1.0) or use
`manual_temperature_decay` from a high start; seed with `random_collect_episode_num>0`; add eps-greedy. The
value/reward plumbing needs no change.

## M3 result — Stochastic MuZero fails the swine cold-start (two configs)

Two runs, matched ~40-min GPU budget, greedy win-rate vs random (deterministic MCTS argmax):

| checkpoint | env-steps | win-rate vs random | behavior | value |
|---|---|---|---|---|
| untrained (iter_0) | 0 | **0.25** (12 g) | quasi-random | ~−0.5 |
| **3.0 pure** iter_10000 | ~25k | **0.00** (30 g, greedy *and* stochastic) | always-mulligan (7×), loses T14–20 | ≈ −0.8 |
| **3.1 shaped** iter_10000 | ~25–34k | **0.00** (30 g) | still always-mulligan | ≈ −0.8 |
| — reference — | | random-v-random **0.535**, PPO 2.9 **0.90** | | |

**Both configs get *worse* than the untrained net** (0% < 25%) and never approach random.

**Root cause — the pre-flagged crux, confirmed.** `predicted_value ≈ −0.8 everywhere`: the value net
learns "every position loses" because collection ~never wins, so there's no gradient toward good play.
MuZero's 50–100-sim search runs over **factored sub-decisions** (a game-turn ≈ several sub-decisions),
so within budget it can't see far enough to find the winning lines needed to bootstrap out of the
losing basin. The greedy policy then collapses to an **always-mulligan attractor** (mulligans to ~0
cards → loses). PPO sidesteps all of this with 500k cheap model-free steps + advantage estimation.

**Did the shaping help?** Mechanically yes, not enough. The gym's own card-dominant PBRS Φ (coef 0.1)
gave the value net a real dense gradient (`reward_loss → 0.4`) and pushed the mulligan visit-split from
3.0's ~**[36,14]** toward **[25,25]/[33,17]** (the Φ cards-term punishing mull-to-0, exactly as
predicted) — but it wasn't enough to flip greedy off mulligan or escape the −0.8 basin at this budget.

**Why M4 is moot.** The headline question was "does learned search fix PPO's chump-blocking?" It can't
be answered here: a policy that mulligans to death and loses every game never reaches meaningful combat,
so there are no block decisions to measure. The negative result *is* the finding.

**What a fair MuZero attempt would likely need (future work, not this window):** (a) **macro-compose a
full engine decision into one search action** so the tree measures game-turns not sub-decisions (the #1
lever — directly attacks the dilution); (b) a **much larger step budget** (PPO needed 500k model-free;
MuZero's model-learning burden makes it slower per useful signal); (c) possibly a **warm-start** from a
behavior-cloned or PPO policy to escape the cold-start basin. None fit the ~3.5h exploratory window.

Both TB runs (`3.0-muzero-swine`, `3.1-muzero-swine-shaped`) are on the shared board (:6006) with
run-notes in each Text tab. Checkpoints kept under each run's `ckpt/`.

---

## TL;DR of the plan

- **Vehicle:** [LightZero](https://github.com/opendilab/LightZero) (opendilab, NeurIPS'23),
  which ships a working **Stochastic MuZero** with a compiled C++ MCTS. Verified installable
  and importable on this box (in an isolated Python 3.11 venv — details below).
- **How we model the game (v1):** as a **single-agent stochastic MDP** — the learner plays
  `agent_seat`, and *everything else* (the opponent's replies **and** the random card draws)
  is folded into the environment's stochastic transitions. This is exactly the shape `MtgEnv`
  already exposes, and exactly what Stochastic MuZero's chance nodes are designed to model. We
  do **not** use LightZero's two-player `board_games` mode in v1 (see "Why not board_games").
- **Self-play:** achieved the PPO way — periodically freeze the current learner and use it as
  the `MtgEnv` opponent. v1 starts vs a **random-legal** opponent (matches the eval baseline),
  then swaps in a frozen-self opponent.

---

## The baseline we're trying to beat (from the lead)

- PPO runs `2.8-swine-500k` / `2.9-swine-500k` (TensorBoard `/tmp/mtgenv_tb`).
- Greedy win-rate vs random ≈ **0.90** (random-vs-random baseline **0.535**), `productive_rate` 1.0.
- **THE KNOWN FAILURE:** at life ≥ 15 the policy still chump-blocks the trampler ~**94–97%** of
  the time (should just take 3). It also rarely gang-blocks (the correct anti-trample play).
  Analyzer: `python/swine_blocks.py`. **If MuZero's lookahead fixes this, that's the headline.**

The swine deck: 25 Forest + 10 Argothian Swine (3/3 trample). Mirror control uses 25 Grizzly
Bears (2/2, no trample) where single-blocking is fine — so bears is the "trample doesn't matter"
control.

---

## M0 verdict — feasibility (GO)

### Compatibility / install

| Thing | Result |
|---|---|
| LightZero implements Stochastic MuZero | **Yes** (`lzero.policy.stochastic_muzero`, C++ ctree `ctree_stochastic_muzero`) |
| Action masks in MCTS | **Native** — env obs is a dict with `action_mask`; MCTS masks illegal actions at root *and* in-tree |
| Two-player / self-play modes | Supported in config (`battle_mode: self_play_mode`, `env_type: board_games`), but we use single-agent in v1 |
| Custom envs | Supported — subclass `ding.envs.BaseEnv`, return the LightZero obs dict |
| Uses Gym or Gymnasium | **Gymnasium ≥1.0** (modern — matches `MtgEnv`) |
| Python 3.14 (this box's only system Python) | **NO.** `requirements.txt` pins `numpy>=1.24.1,<2` (no 3.14 wheels; 1.x tops out at 3.12) and wants `numba` (lags new Pythons). Forces an older Python. |
| Fix | **Isolated `uv` venv on Python 3.11.15** (uv provisions it; system has no 3.8–3.13). |
| C++ MCTS compiles under GCC 16.1.1 | **Yes** — `pip install LightZero` (v0.2.0) built `ctree_stochastic_muzero` cleanly. |
| torch + CUDA in the 3.11 venv | **Yes** — torch 2.12.1+cu130, `cuda.is_available()` True. |
| DI-engine (`ding`) | v0.5.3 installed & imports. |
| numpy | 1.26.4 (satisfies `<2`). |

`numba` is **optional** (only speeds up the replay-buffer segment tree). LightZero warns but runs
without it. Can `uv pip install numba` later for a minor collection speedup.

### The linchpin: `mtg_py` works across Python versions

The Rust engine extension `mtg_py` is built with **PyO3 `abi3-py39`** (the CPython *stable ABI* —
see `crates/mtg-py/Cargo.toml`). One build runs on **any** CPython ≥ 3.9. So the same engine can
be imported from the Python 3.11 LightZero venv *and* the Python 3.14 training venv. We build a
wheel with `maturin` and `uv pip install` it into this venv (M1). No engine changes needed.

### Concrete dimensions (swine, measured)

- `action_dim` = **98** (the factored `Discrete` action space).
- Flattened observation vector = **2650** floats = 1966 (raw `obs_spec` tables: globals,
  battlefield, hand, stack) + 684 (per-row card-id one-hots; swine vocab = 3 unique cards).
- → Stochastic MuZero **MLP** model: `observation_shape=2650`, `action_space_size=98`, `to_play=-1`.

---

## The adapter design (M1) — how the mapping works

### One Gym step = one *factored sub-decision*

`MtgEnv` decomposes each engine decision into a sequence of sub-decisions (pick request → pick
target → pick mana → …), and every `step` is one sub-decision from the same `Discrete(98)` space
with a per-step legality mask. **Search therefore operates over sub-decisions, not whole moves.**

**Tree-depth implication (important, honest caveat):** a single "turn" (untap → draw → main →
attack/block → …) spans *many* sub-decision steps. With N MCTS simulations the tree only reaches
a handful of sub-decisions ahead, so within a fixed budget the search sees *fewer game-turns* of
lookahead than a move-level search would. Whether that's enough to reach the *consequence* of a
block (take-3 vs trade-into-trample) is exactly the open question this experiment answers. Two
mitigations exist and are deferred past v1: (a) larger sim budget, (b) macro-composing a full
engine decision into one search action. **v1 does NOT macro-compose** — it searches raw
sub-decisions and leans on the learned value function for multi-turn credit.

### Why single-agent, not `board_games` (v1 decision)

`MtgEnv` is two-player, stochastic (draws), and **imperfect-information** (obs = the deciding
seat's info-state). LightZero's `board_games` self-play machinery (Gomoku/Connect4-style) assumes
**perfect information and deterministic dynamics** — it has *no shipped example* combining
two-player + stochastic chance nodes + hidden info, and the stochastic ctree's turn-alternation
for that combo is unproven. Forcing it there is high-risk for v1.

Instead we use the shape `MtgEnv` already gives the learner: a **single-agent** POMDP where the
opponent's policy is part of the environment. `env_type='not_board_games'`, `to_play=-1`. This is
the *same* framing the PPO baseline trains under, so the comparison is apples-to-apples, and it's
the framing Stochastic MuZero's single-player examples (2048, backgammon) use.

### Chance / afterstate mapping (Stochastic MuZero)

Stochastic MuZero factors a transition as
`state --(action)--> afterstate --(chance outcome)--> next state`, where the *chance* transition
is a learned categorical code (VQ-VAE-style) inferred from consecutive observations — **we do not
have to hand it explicit chance codes.**

Our mapping:
- **afterstate** = the position *immediately after* the learner commits its sub-decision action,
  *before* the environment resolves what happens next.
- **chance outcome** = everything the environment does before the learner's next decision: the
  opponent's replies **and** the random card draw(s). From the learner's seat those are
  indistinguishable "environment stochasticity," which is precisely what the chance node absorbs.

So no engine change is needed to expose draws — the stochastic model *learns* the outcome
distribution from observed (afterstate → next obs) pairs during training.

**Verified against LightZero's implementation (not just docs):** LightZero's Stochastic MuZero has
a config flag `use_ture_chance_label_in_chance_encoder` [sic].
- With it **True** (how the shipped 2048 example runs), the env must emit a ground-truth `chance`
  code with a fixed, enumerable `chance_space_size` (2048's is "which empty cell got which tile").
  **Our stochasticity is not enumerable that way**, so we do NOT use this mode.
- With it **False** (the ICLR-2022 paper's actual method), the replay buffer never reads a `chance`
  label, and a learned **VQ `ChanceEncoder`** (`OnehotArgmax` straight-through estimator over a
  `chance_space_size` codebook — and there's an **MLP backbone** for vector obs, exactly our case)
  infers the chance code from consecutive observations. This is the mode we use.
- Practical consequence for the adapter: set `use_ture_chance_label_in_chance_encoder=False`, pick a
  small `chance_space_size` (VQ codebook size — a hyperparameter, e.g. 4–8), and the env's obs dict
  needs only `{observation, action_mask, to_play}` — no `chance` field required.

### Obs flattening

`MtgEnv`'s `Dict` obs (globals / bf_feat / hand_feat / stack_feat + `*_ids` + card-id one-hots)
is concatenated to a single **2650-float** vector for the MLP model. The card-id one-hots (the
interpretable card identity) are kept — they're the part that lets the net tell a 3/3 trample
Swine from a 2/2 Bear. (A structured/CNN encoder is a later refinement; v1 uses the flat MLP.)

---

## Files

| File | What it is |
|---|---|
| `swine_lightzero_env.py` | `MtgSwineEnv(BaseEnv)` — wraps `MtgEnv`, flattens obs → 2650 vec, emits `{observation, action_mask, to_play=-1, timestep=-1}`. |
| `swine_stochastic_muzero_config.py` | Stochastic MuZero **MLP** main/create config. `--smoke` = tiny; default = M3 real. Run it to train. |
| `lz_patches.py` | In-memory monkeypatch for a LightZero v0.2.0 stochastic-muzero bug (see below). Imported by the config. |
| `smoke_env.py` / `smoke_model.py` | M1 wiring checks (env reset/step; model forward at swine dims). |
| `eval_muzero_swine.py` | M4 harness: greedy win-rate vs random + chump-block/gang self-mirror analysis. Built & plumbing-validated; runs once M3 gives a checkpoint. `--latent-state-dim` matches the ckpt (real=256). |

## Integration fixes discovered during M1/M2 (so a masked, factored, single-agent env trains)

Three concrete things were needed to make LightZero's Stochastic MuZero accept this env — all are
adapter-side (no engine changes), documented so the result is reproducible:

1. **`action_type='varied_action_space'`** (policy config) — **the important one.** Our legal-action
   set varies per node (2..98). The default `'fixed_action_space'` (Atari) stores raw
   variable-length MCTS visit distributions → the policy-target array is inhomogeneous → crash. The
   `varied_action_space` path scatters each distribution into a fixed length-98 vector via the
   legal-action indices (the same setting LightZero's own board-game configs use).
2. **Scalar (0-d) step reward** — the env must return `to_ndarray(float(r))`, not shape `(1,)`. The
   replay buffer pads reward targets with `np.array(0.)` (0-d); mixing `(1,)` + `()` → inhomogeneous.
3. **`lz_patches.py` — `timestep` kwarg drift (a genuine LightZero v0.2.0 bug).** The
   collector/evaluator call `policy.forward(..., timestep=...)`; `MuZeroPolicy` absorbs it via
   `**kwargs` but `StochasticMuZeroPolicy._forward_collect/_forward_eval` were never given `**kwargs`,
   so they crash. `timestep` isn't used inside `forward` for stochastic muzero, so the patch just
   drops it before delegating. (We also add `timestep=-1` to the obs dict to silence a benign warning.)

## M2 smoke result (CPU, `--smoke`, no GPU)

Full pipeline (collector + C++ ctree MCTS + VQ chance encoder + learner + replay buffer) trains
end-to-end, exit 0, 36 train iterations, checkpoints saved. First-iter losses finite & typical for
MuZero-with-categorical-support at init: `total 86.7`, `policy 26.4`, `reward 32.0`, `value 38.4`,
`consistency ≈0`, and the **stochastic-specific `afterstate_policy_loss` / `afterstate_value_loss` /
`commitment_loss` all present** — i.e. the afterstate/chance machinery is actually engaged. No NaN/inf.

## Environment setup

```bash
# from experiments/stochastic_muzero/
uv venv .venv --python 3.11          # already done (CPython 3.11.15)
source .venv/bin/activate
uv pip install LightZero             # already done (v0.2.0 + DI-engine + torch cu130)
# (optional) uv pip install numba    # minor segment-tree speedup

# M1: build the abi3 engine wheel and install it here, plus the pure-Python gym layer:
#   (cd ../../crates/mtg-py && maturin build --release) ; uv pip install <wheel>
#   uv pip install -e ../../python      # mtgenv_gym (pure Python)
```

The 3.11 venv is **separate** from `python/.venv` (3.14, the PPO training env) — they don't
interfere. `mtg_py` (abi3) is import-compatible with both.

---

## Milestones (report to lead at each)

- **M0 — feasibility** ✅ GO. LightZero installs & imports (incl. C++ stochastic MCTS) in an
  isolated Py3.11 venv; `mtg_py` is abi3 so it drops in; dims measured (obs 2650, actions 98).
- **M1 — adapter.** ✅ `MtgSwineEnv(BaseEnv)` wrapping `MtgEnv`: flatten obs → 2650 vec, surface
  `action_mask`, `to_play=-1`, single-agent vs random opponent. Engine wheel built+installed here.
  Env + model wiring smokes pass.
- **M2 — smoke train.** ✅ Tiny config trains end-to-end on swine (CPU), no crash, losses sane,
  checkpoints saved. Needed the 3 integration fixes above.
- **M3 — real run** (GPU). ✅ *Concluded — negative.* Two matched-budget runs (`3.0` pure sparse,
  `3.1` PBRS-shaped), each ~40 min / ~25–34k env-steps on GPU. Both collapse at the sub-decision
  cold-start (0% win vs random, always-mulligan attractor, value ≈ −0.8). See "M3 result" up top.
- **M4 — evaluate.** ⏹️ *Moot / not run.* The harness (`eval_muzero_swine.py`) is built and validated,
  but the judgment comparison (chump-block rate at life ≥ 15, gang rate) requires a policy that
  *plays* — MuZero here never reaches competent combat (mulligans to death), so there are no block
  decisions to measure and no fair head-to-head vs PPO. Documented as the reason, not skipped silently.

**Outcome: honest negative — a success state.** Stochastic MuZero (LightZero) is fully wired to the
swine env (pipeline, masking, VQ chance, self-play framing all work), but at a matched ~40-min budget
it does **not** learn competent play, so it can't be compared to PPO on combat judgment. The blocker is
the pre-flagged sub-decision-lookahead-dilution + sparse-reward cold-start, confirmed across two
configs. Future work to give MuZero a fair shot is listed under "M3 result" (macro-compose decisions,
much larger budget, warm-start).
