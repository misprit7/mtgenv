# Stochastic MuZero on the swine environment

**Exploratory workstream.** Question: does learned MCTS lookahead (Stochastic MuZero) beat
the model-free PPO baseline on the swine matchup — *especially* on combat judgment, where PPO
has a known, stubborn failure (it chump-blocks the trampling 3/3 even at high life)?

Everything here is **isolated**: this dir has its own venv, README, and `.gitignore`. It only
*reads/imports* `python/mtgenv_gym/` and `crates/` — it never modifies them.

> Status: **M0 (feasibility) — DONE, verdict GO.** See "M0 verdict" below. M1 (adapter) next.

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
- **M1 — adapter.** `SwineLightZeroEnv(BaseEnv)` wrapping `MtgEnv`: flatten obs → 2650 vec,
  surface `action_mask`, `to_play=-1`, single-agent vs random/frozen opponent. Build+install the
  engine wheel here. Smoke that LightZero can `reset/step` it.
- **M2 — smoke train.** Tiny config (few sims, small net) trains on swine without crashing; loss
  curves sane. CPU or brief GPU.
- **M3 — real run** (GPU, coordinate via lead). Comparable-budget run (a few hours; note: matching
  PPO 500k ≈ 20 min wall-clock is likely too little for MuZero — budget documented honestly).
  Versioned TB run.
- **M4 — evaluate.** (a) win-rate vs random-legal (same 500-game protocol; vs 0.535 / PPO 0.90);
  (b) head-to-head vs PPO 2.9 if feasible; (c) **the judgment metrics** — `swine_blocks.py`-style
  chump-block rate at life ≥ 15 and gang rate on greedy MuZero play. Honest comparison table + verdict.

**Honest negative results are a success state.** If Stochastic MuZero underperforms PPO at matched
budget, this README will say so plainly, with the numbers.
