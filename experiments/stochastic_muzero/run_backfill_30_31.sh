#!/bin/bash
cd /home/xander/dev/p-mtg/mtgenv/experiments/stochastic_muzero
export PYTHONPATH=../../python
PY=./.venv/bin/python
echo "=== BACKFILL 3.0-muzero-swine (pure collapse story) ==="
$PY muzero_observability.py --config swine_stoch --ckpt-dir tb/3.0-muzero-swine/ckpt \
  --run-log m3_run.log --logdir tb/3.0-muzero-swine/log/serial \
  --run-name 3.0-muzero-swine --games 50 --sampled-games 20 --latent 256 --sims 50
echo "=== BACKFILL 3.1-muzero-swine-shaped (weak-shaping collapse) ==="
$PY muzero_observability.py --config swine_stoch --ckpt-dir tb/3.1-muzero-swine-shaped/ckpt \
  --run-log m3_shaped.log --logdir tb/3.1-muzero-swine-shaped/log/serial \
  --run-name 3.1-muzero-swine-shaped --games 50 --sampled-games 20 --latent 256 --sims 50
echo "=== 3.0/3.1 BACKFILL DONE ==="
