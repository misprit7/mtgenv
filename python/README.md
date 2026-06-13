# mtgenv_gym — Python RL environment (GYM_PLAN L4)

A Gymnasium env over the `mtg-core` engine via the `mtg_py` PyO3 extension (`crates/mtg-py`).

- **Milestone 0:** PyO3 boundary + random self-play (thousands of legal games, conservation holds).
- **Milestone 1:** structured per-entity observation (with `grp_id` card embeddings), a factored
  fixed-width action space with env-side autoregressive decomposition + legality mask, sparse
  terminal reward, and a `MaskablePPO` agent that **beats a random opponent**.

## Setup (uv)

```bash
# from the repo root — the Python deps are declared in python/pyproject.toml
uv venv .venv --python 3.14            # create the venv (any CPython 3.9+ works via abi3)
uv pip install -e "python[dev]"        # numpy, gymnasium, sb3-contrib (torch), tensorboard, pytest, maturin

# build + install the Rust extension `mtg_py` into the venv
# (abi3 forward-compat lets the build target the box's newer-than-known CPython 3.14)
PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 .venv/bin/maturin develop --release -m crates/mtg-py/Cargo.toml
```

`uv run --python .venv …` or `.venv/bin/python …` runs anything below; `PYTHONPATH=python` puts the
top-level scripts (`train.py`, `benchmark.py`) and `mtgenv_gym` on the path.

## Run

```bash
# fast smoke (random self-play legality + conservation + env Dict obs)
PYTHONPATH=python .venv/bin/python -m pytest python/tests/test_smoke.py -q

# learning-sanity (trains ~20s, asserts win-rate beats random)
PYTHONPATH=python .venv/bin/python -m pytest python/tests/test_learning.py -q

# throughput / conservation report
PYTHONPATH=python .venv/bin/python python/benchmark.py --deck demo --games 3000

# train + report win-rate vs random (the M1 exit criterion)
PYTHONPATH=python .venv/bin/python python/train.py --deck burn_vs_bears --timesteps 60000 --eval-games 400

# …with TensorBoard curves (PPO losses + periodic eval/mean_reward = win-rate signal)
PYTHONPATH=python .venv/bin/python python/train.py --deck burn_vs_bears --timesteps 60000 \
    --tensorboard runs/ --tb-eval-freq 4000
tensorboard --logdir runs/
```

## Layout

- `mtgenv_gym/env.py` — `MtgEnv(gym.Env)`: single agent (`agent_seat`) vs a fixed opponent
  (random by default; a frozen checkpoint plugs in for M2 self-play). `gym.spaces.Dict`
  observation + `Discrete` action space, both **read from the extension** (`PyGame.obs_spec()` /
  `PyGame.action_dim()`) so the Rust encoder/codec stay swappable. Factored mask in
  `info["action_mask"]` / `action_masks()`.
- `mtgenv_gym/policy.py` — `EntityExtractor`: a DeepSets features extractor that embeds each
  entity's `grp_id`, runs a shared per-row MLP, and masked-mean-pools each table (permutation
  invariant) before the actor/critic heads.
- `mtgenv_gym/selfplay.py` — low-level random self-play driver (no Gym dep) for the smoke/benchmark.
- `train.py` — MaskablePPO training + greedy win-rate eval vs random (`train_and_eval` is reusable).
- `tests/` — `test_smoke.py` (legality/conservation/env), `test_learning.py` (beats random).

The observation encoder (`obs.rs` + `layout.rs`) and action codec (`codec.rs`) live in Rust; this
package is plumbing + the policy network.
