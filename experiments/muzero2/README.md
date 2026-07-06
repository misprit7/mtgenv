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

- (100k-gate numbers + curves + verdict to be recorded here)
