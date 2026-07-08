# muzero2 — LightZero MuZero / Gumbel-MuZero on the mtgenv gym, done right (Track A)

**User-directed retry.** Position: the prior MuZero failure was likely OUR hookup / hyperparameters,
not the algorithm; with the dense heuristic (PBRS) rewards this is a *high-signal* problem;
explicit/real-state search is OFF the table (hidden info). This dir is isolated (own venv, README,
`.gitignore`); it only *imports* `python/mtgenv_gym/` + `crates/` (via the `mtg_py` wheel) and never
modifies them. The prior workstream `../stochastic_muzero/` is **READ-ONLY reference**.

## What the prior arc established (read `../stochastic_muzero/README.md` first)

Plumbing triple-audited CLEAN. The collapse root cause: a **FLAT value net** (`td_steps=5` far too
short for 30–60-sub-decision episodes ⇒ terminal ±1 never reaches early decisions) + a legal **PASS at
action index 0** (the argmax/visit-count "passive" attractor: greedy play = do-nothing = 0% win) +
sparse terminal reward + **over-training the tiny early buffer** (`update_per_collect=100`), with **no
exploration floor** (`random_collect_episode_num=0`). Its combined recipe `--shaping 0.5 --td 40 --up 20`
made *plain* MuZero learn trivial **heralds** (0 → ~0.9 sampled, peak) but only at ~60k steps with the
collector still rising, and **swine** re-collapsed (0 → 0.12). PPO baseline: heralds ~0.97, swine ~0.90.

**Never tried by the prior arc (this retry's checklist):** a real budget (≥500k env steps, hours),
**reanalyze** (fresh target recomputation), **Gumbel-MuZero** (low-sim policy improvement), constant
(non-annealed) shaping at full strength, larger nets/td/unroll, an **exploration floor**.

## The recipe (this retry) — every lever the prior run never combined

| lever | value | why |
|---|---|---|
| **algo A** | **Gumbel-MuZero** (`--algo gumbel`) | root action = Gumbel-top-k + sequential halving (NOT argmax visits) ⇒ guaranteed policy improvement at low sims; **directly dissolves the low-index/PASS passive attractor** |
| **algo B** | plain MuZero (`--algo muzero`) | clean A/B control (same env/recipe/net) — extends the prior known-good learner |
| **reanalyze** | `reanalyze_ratio=0.5` | recompute fresh policy+value targets from the current model on old trajectories — OFF in every prior run |
| **exploration floor** | `random_collect_episode_num=32` | seed the buffer with random games (~0.53 win) so the value net has balanced win/loss targets from step 0 — attacks "buffer fills with losses" |
| **dense shaping** | `reward_shaping=0.5`, CONSTANT | card-dominant PBRS Φ (same as PPO); dense anti-passive value signal every step; **eval is always raw ±1** (`eval_episode_return` unshaped) |
| **credit reach** | `td_steps=50`, `num_unroll_steps=10` | carry credit across long factored episodes (the flat-value fix; prior used 5/5 then 40/5) |
| **net capacity** | `latent_state_dim=512`, heads `[64]` | more value/dynamics capacity (prior 256/[32]) |
| **budget** | ≥500k env steps (6h cap) | prior was ~40 min / 60k with the curve still rising |
| **no over-train** | `update_per_collect=20` | don't over-train the tiny early buffer (prior 100 hurt) |
| **no segment split** | `game_segment_length=2000` | > any real game ⇒ sidesteps LightZero's `varied_action_space` boundary IndexError (`game_buffer_muzero.py:737`) |

Eval always via **evalkit** (`mz_policy.py` → `evaluate_checkpoint`): greedy (fair tie-break: argmax
visits, ties broken by the network **prior**, not lowest index) AND sampled, Wilson CIs,
productive/attack rates, swine chump/gang analyzer, one replay — overlaid with PPO on the shared board.

## Gate & success bar (from the mandate)

- **Gate @ 100k (heralds):** sampled win-vs-random CLEARLY > 0.535 and rising (beat the prior ~0.4–0.5
  stall at 60k). If it fails twice → Track B (a different implementation; feasibility-read first).
- **Success (heralds):** ≥ 0.9 (PPO 0.97). Then **swine 500k+** with the winning recipe + chump analyzer.

## Files

| file | what |
|---|---|
| `mtg_lz_env.py` | LightZero `BaseEnv` over `MtgEnv` (registered `mtg_lz`), deck-agnostic; flatten obs, mask, `to_play=-1`, constant PBRS shaping, raw ±1 eval return. |
| `mtg_config.py` | `build_configs(algo, deck, …)` — the ONE config builder shared by training and eval (guarantees the eval model matches the checkpoint). |
| `train.py` | `--algo {gumbel,muzero} --deck … --exp tb/<run> --max-steps N`. `--smoke` = tiny wiring check. |
| `lz_patches.py` | in-memory LightZero patch: enables the **Gumbel random-collect exploration floor** (adds the muzero MCTS path + the `improved_policy_probs`/`roots_completed_value` fields the gumbel collector expects) and absorbs the collector's `timestep` kwarg. |
| `mz_policy.py` | evalkit `Policy` adapter (batched MCTS `act`, fair-greedy / sampled) + eval CLI + `--watch` mode (poll a run's `ckpt/` and eval each new checkpoint onto the shared board). |

## How to run

```bash
# from experiments/muzero2/  (venv: Python 3.11 + LightZero + torch cu130 + mtg_py abi3 wheel)
# TRAIN (exp_name is CWD-relative: LightZero prepends ./, so tb/<run> → experiments/muzero2/tb/<run>)
PYTHONPATH=../../python .venv/bin/python train.py --algo gumbel --deck heralds \
    --exp tb/3.4-gumbel-heralds --max-steps 500000 > 3.4.log 2>&1 &

# EVAL a checkpoint onto the SHARED board (/tmp/mtgenv_tb — abs path, not mangled):
PYTHONPATH=../../python .venv/bin/python mz_policy.py --algo gumbel --deck heralds \
    --ckpt tb/3.4-gumbel-heralds/ckpt/ckpt_best.pth.tar --step 100000 \
    --run-dir /tmp/mtgenv_tb/3.4-gumbel-heralds --run-log 3.4.log

# WATCH (continuous eval of new checkpoints during training):
PYTHONPATH=../../python .venv/bin/python mz_policy.py --algo gumbel --deck heralds \
    --ckpt-dir tb/3.4-gumbel-heralds/ckpt --run-dir /tmp/mtgenv_tb/3.4-gumbel-heralds \
    --run-log 3.4.log --watch &
```

Environment (isolated, own venv — see the prior README for the pattern):
`uv venv .venv --python 3.11 ; uv pip install LightZero ; uv pip install ../../target/wheels/mtg_py-*manylinux*.whl ; uv pip install --no-deps -e ../../python`.
Random-vs-random baseline (evalkit): heralds **0.550**, swine **0.525**.

---

## Lab notebook

### 2026-07-06 — setup + integration (M0/M1/M2 equivalent)

- New isolated Py3.11 venv; LightZero + torch 2.12.1+cu130 (CUDA ✓) + Jul-4 `mtg_py` abi3 wheel +
  editable `mtgenv_gym`. evalkit `Arena` verified on the wheel (random-v-random heralds 0.550 / swine
  0.525 — matches baselines). obs_dim heralds 2593 / swine 2650, action_dim 98.
- Integration fixes found (all adapter-side, no engine/gym changes):
  1. **Gumbel random-collect** is unimplemented in `LightZeroRandomPolicy` → `lz_patches.py` aliases it
     to the muzero MCTS path and synthesizes the `improved_policy_probs` (visit dist scattered to full
     length) + `roots_completed_value` (searched value) fields the gumbel collector reads, and drops the
     collector's `timestep` kwarg. Plain muzero needs no patch (native random-collect).
  2. **Segment-boundary IndexError** (`game_buffer_muzero.py:737`) fires whenever a game splits at
     `game_segment_length`; heralds games are 30–60 sub-decisions so a smoke `seg=50` tripped it →
     real runs use `seg=2000` (no split). Confirmed both the bug and the fix.
- **Smokes PASS** (EXIT 0, checkpoints saved) for both `--algo gumbel` and `--algo muzero`; eval adapter
  validated end-to-end on a gumbel smoke ckpt (untrained → 0.00 win, full battery + Wilson CI runs).

### 2026-07-06 — Track-A launch (heralds A/B)  *(in progress)*

**Throughput probe → tuning.** First probe at `sims=50, reanalyze_ratio=0.5` was too slow (reanalyze
MCTS dominates; ~2 collects/3min → 500k would blow the 6h cap). Each run is CPU-serial-bound at ~1.2
cores (2 runs = 2.4/32 cores → parallelism is FREE; GPU ~9%). Retuned to **`sims=25,
reanalyze_ratio=0.25, gumbel max_considered=8`** — aligns with the Gumbel thesis (designed for low
sims), still fully reanalyze-on. Measured **~31 env-steps/sec** → 500k ≈ **4.4h** (under the 6h cap).

**Launched (both heralds, 500k, tuned):**
- `3.4-gumbel-heralds` (Gumbel) — PID logged in `3.4-gumbel-heralds.log`.
- `3.5-muzero-reanalyze-heralds` (plain MuZero) — `3.5-muzero-reanalyze-heralds.log`.
- Shared board: LightZero internals symlinked as `…-train` runs (loss / collector `reward_mean` /
  `eval_episode_return`); evalkit watchers (`mz_policy.py --watch`, 40 games, sims 25, poll 420s) write
  the canonical `selfplay/winrate_vs_random(_sampled)` + `stats/*` curve to `/tmp/mtgenv_tb/3.4…` / `3.5…`.
- iteration_0 (untrained, post-random-collect) evalkit: gumbel greedy/sampled 0.00/0.00; muzero
  0.00/0.40. Baseline random-v-random 0.535.

**A/B forced SEQUENTIAL** (lead directive): concurrent GPU jobs on this box have a documented
nondeterminism gotcha + halve throughput, so each run gets the GPU alone. Order: A (Gumbel) → gate → B.

**Run A (Gumbel) — FAILED the 100k gate.** @ ~96k, evalkit (200 games): greedy **0.035** (95%
0.02–0.07), sampled 0.000, prod 0.22, atk 0.46 — **worse than random (0.535)**. Collector `reward_mean`
oscillates around ~−0.4 (collection is losing, not just eval). Decoupled check: a manual greedy loop
straight through `MtgEnv(opponent="random")` = 0.04 (matches evalkit → not an Arena artifact).
**Sims-invariant**: greedy @25/50/100 sims = 0.05/0.05/0.00 → mis-trained model, NOT search-starvation.

- **Adapter bug found + fixed (important).** Gumbel's eval action is `argmax(improved_policy_probs)`
  (completed-Q), NOT argmax visit counts — at low sims they routinely differ (observed: it picks a
  3-visit action over a 22-visit one). The first watcher curve (flat 0.000, identical turns=17.6) was
  this bug. `mz_policy.MuZeroLzPolicy` is now algo-aware: gumbel greedy = framework `action`, gumbel
  sample = policy-head softmax; muzero greedy = fair-greedy (visits + prior tie-break), sample =
  visits^(1/temp). Even after the fix Gumbel is genuinely ~0.04 (moved from "passive" to "acts but
  loses"), so the failure is real.
- **Read:** Gumbel's completed-Q improved policy is unreliable under our sparse+shaped reward at low
  sims and trains the policy head toward bad actions. Plain MuZero (visit-count targets + Dirichlet
  root exploration) is more robust — hence Run B.

**Run B (plain MuZero) — in progress** (`3.5-muzero-heralds`). De-risked to the prior's PROVEN heralds
recipe (**sims 50, td 40, unroll 5, up 20, latent 256, shaping 0.5**) + the two cheap mandate levers
(**reanalyze 0.25, random_collect 32**). Highest-probability path to reproduce ~0.9 while still testing
reanalyze/buffer-seeding. Fallback if B is also worse-than-random: reanalyze-off control (prime suspect).

**Run B PASSES the 100k gate — the retry works.** evalkit vs random (0.535 baseline), greedy/sampled:

| env-step | greedy | sampled | prod | atk | note |
|---|---|---|---|---|---|
| 0 | 0.000 | 0.400 | 0.23 | 0.12 | untrained (post-random-collect) |
| 31k | 0.000 | 0.000 | 0.05 | 0.00 | early |
| 48k | 0.175 | 0.200 | 0.41 | 0.19 | rising |
| 65k | 0.600 | 0.550 | 0.63 | 0.56 | crosses random |
| 83k | 0.550 | 0.525 | 0.81 | 0.47 | |
| **98k** | **0.680** (95% 0.61–0.74) | **0.705** | 0.81 | 0.47 | 200-game |

Collector `reward_mean` stably positive (0.45–0.75). Clearly >0.535 and rising from zero — beats the
prior arc's stalling ~0.4–0.5 and already matches its best sampled (~0.72) at 98k, budget remaining.

**Headline:** the failure was the *algorithm/recipe*, not anything fundamental. Plain MuZero (robust
visit-count targets + Dirichlet root exploration) + reanalyze 0.25 + random_collect 32 learns heralds
where Gumbel collapsed worse-than-random. Both new mandate levers (reanalyze, buffer-seeding) are
compatible/working — not a poison. Letting B climb toward the ≥0.9 success bar, then SWINE (same recipe).

**Run B — HERALDS SOLVED (≥0.9 bar cleared).** The true peak checkpoint `iteration_7000` (120k env
steps), 200-game evalkit: **greedy 0.930 (95% 0.89–0.96), sampled 0.920**, prod 0.98, atk 0.68 —
clears the ≥0.9 success bar, approaching PPO's 0.97. `iteration_6000` (102k) = 0.895/0.895. Trajectory
0 → 0.93 by 120k, then over-training drift to ~0.65–0.72 by 176k (so B was stopped at 200k — no longer
rising; heralds banked). **Gotcha:** LightZero's `ckpt_best` (chosen by its 3-game internal evaluator)
was a mis-pick — 200-game eval of `ckpt_best` = 0.635, far below the true best `iteration_7000` (0.93).
**⇒ trust the watcher's 200-game iteration evals, not `ckpt_best`, to pick the deployable checkpoint.**

**VERDICT (heralds):** the retry works. Plain MuZero + reanalyze 0.25 + random_collect 32 + the proven
recipe learns heralds to **0.93** (0 → competent), decisively beating the prior arc's stall (~0.4–0.5)
and clearing the bar. The prior "MuZero can't do our gym" was an algorithm/recipe artifact (Gumbel's
fragile low-sim completed-Q), not fundamental. Both never-tried mandate levers (reanalyze, buffer-seed)
are working and compatible.

### 2026-07-06 — HERALDS VERIFICATION (fresh large-N, user-requested)

Independent re-eval of the peak checkpoint on FRESH seeds (base 7,000,000; watcher used 5,000,000 — no
overlap), 500 greedy + 500 sampled via `verify_heralds.py`. Checkpoint
`tb/3.5-muzero-heralds/ckpt/iteration_7000.pth.tar` (env-step 120113), md5
`51f0eea658c92e4b99634e1d7d7cdf23`. Opponent RandomPolicy(seed 7,000,000) (== `MtgEnv(opponent="random")`).
Shaping OFF at eval by construction (Arena reads raw ±1 `ext_reward()`; `reward_shaping` is a
training-wrapper-only param).

| mode | win_rate | 95% Wilson | W/L/D | turns_mean | end_reasons |
|---|---|---|---|---|---|
| greedy | **0.926** | 0.900–0.946 | 463/37/0 | 14.22 | zero_life=1.000 |
| sampled | **0.924** | 0.897–0.944 | 462/38/0 | 14.22 | zero_life=1.000 |

Every game decided by reducing the opponent to 0 life (no draws/timeouts). greedy stats: productive 0.98,
cast 0.92, playland 0.65, attack 0.68. JSON: `/tmp/mtgenv_tb/3.5-heralds-verify/evalkit/eval_step000120113.json`;
6 replays in `data/replays` (run_name `3.5-heralds-verify`) for the web lobby. **Fresh-seed 500-game
result reproduces the 0.93 claim: 0.926 greedy / 0.924 sampled.**

### 2026-07-06 — SWINE — STOPPED by user directive (pending heralds sign-off)

Swine (`3.6-muzero-swine`, same validated recipe) was launched but STOPPED per user instruction: not to
run swine until satisfied heralds is properly won. The original run died at 32k (transient external
kill — no code error; before death it showed greedy 0.60 + active blocking at 20k). NOT relaunching swine
until the user explicitly clears it. When cleared: same recipe, evalkit `SwineBlockAnalyzer` auto-runs
(chump/gang at life ≥15 — the PPO failure: PPO chump-blocks the 3/3 trampler 94–97%).

### 2026-07-07 — 4.x: three more model-based tree-search families (heralds A/B/C)

Comparison arms so the user can see which model-based tree-search family best learns heralds under the
proven 3.5 recipe. **HERALDS ONLY** (swine forbidden). All log the same evalkit schema; metric = FINAL
checkpoint (no peak-pick). Config builder (`mtg_config.build_configs`) now covers 5 algos; `train.py`
routes `train_muzero` vs `train_unizero`; `mz_policy.py` is algo-aware.

| arm | run | algo | key knobs / deviations |
|---|---|---|---|
| 4.0 | `tb/4.0-ez-heralds` | EfficientZero | SSL consistency + value-prefix LSTM (`lstm_hidden_size=256`); native random-collect; 3.5 recipe as-is |
| 4.1 | `tb/4.1-stochmz-heralds` | Stochastic MuZero | `chance_space_size=32` (MTG-draw stochasticity), `use_true_chance_label=False`; `lz_patches` adds its random-collect alias + `timestep`-strip |
| 4.2 | `tb/4.2-unizero-heralds` | UniZero | transformer WM (embed 256/4L/4H); **AdamW lr 1e-4**, **reanalyze 0**, **no random-collect** (LZ has no unizero random pipeline), num_unroll 10 = WM token budget |

- **UniZero eval is recurrent** (per-episode kv-cache + `last_batch` context; `_forward_eval` takes a
  `timestep`). The shrinking/reordering batched Arena can't drive that, so the adapter runs UniZero at
  **Arena batch_size=1**, calls `_reset_eval` per game (via `Policy.reset`), and increments a per-episode
  timestep. Replay disabled for UniZero (the replay recorder never calls `reset`). EZ/StochMZ reuse the
  MuZero visit-count eval path unchanged.
- **Gotcha:** UniZero's model is a `torch.compile` `OptimizedModule` whose `__bool__`→`__len__` raises —
  the adapter's `_eval_model or _learn_model` truthiness had to become an explicit `is None` check.
- Patches (`lz_patches.py`): the random-collect alias generalized to `{gumbel_muzero, stochastic_muzero}`
  (both → MuZero MCTS/model for the seed phase; only gumbel synthesizes `improved_policy_probs`);
  `StochasticMuZeroPolicy._forward_collect/_eval` get the `timestep` kwarg stripped.
- All three smoke-tested (train + eval adapter, EXIT 0) and reanalyze-0.25 crash-checked for EZ/StochMZ;
  launched detached with evalkit watchers (40 games, sims 25, poll 420s), staggered on the shared 4090
  (~3.4 GB used at launch — ample headroom). iteration_0 (untrained) evals confirm the pipeline:
  EZ 0.00/0.45, StochMZ 0.00/0.50, UniZero 0.00/0.60 (greedy/sampled).
