# mtgenv_gym — Python RL environment (GYM_PLAN L4)

Milestone 0: a Gymnasium env over the `mtg-core` engine via the `mtg_py` PyO3 extension
(`crates/mtg-py`). Random self-play runs thousands of legal games to termination with
non-empty action masks and card/zone conservation.

## Setup

```bash
# from the repo root
python3 -m venv .venv && source .venv/bin/activate
pip install maturin numpy gymnasium pytest

# build + install the Rust extension `mtg_py` into the venv
# (PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 lets the abi3 build target a newer-than-known CPython)
PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 maturin develop --release -m crates/mtg-py/Cargo.toml
```

## Run

```bash
# smoke test (fast, a few hundred games per deck)
PYTHONPATH=python pytest python/tests -q

# throughput / exit-criteria report (thousands of games)
PYTHONPATH=python python python/benchmark.py --deck demo --games 3000
```

## Layout

- `mtgenv_gym/env.py` — `MtgEnv(gym.Env)`: `reset`/`step`, obs from the Rust encoder, action
  mask in `info["action_mask"]`. Observation width and action vocabulary are read from the
  extension (`PyGame.obs_dim()` / `PyGame.action_dim()`), **not hard-coded** — the encoder and
  codec are swappable on the Rust side (milestone 1) without changing this layer.
- `mtgenv_gym/selfplay.py` — low-level random self-play driver (no Gym dep); the throughput +
  conservation harness used by the smoke test and benchmark.
- `tests/test_smoke.py` — the milestone-0 exit-criteria assertions.
- `benchmark.py` — games/s + conservation report.

The decision boundary, observation encoding, and action codec are all in Rust
(`crates/mtg-py/src/{game,obs,codec}.rs`); this package is plumbing.
