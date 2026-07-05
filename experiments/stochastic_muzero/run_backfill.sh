#!/bin/bash
cd /home/xander/dev/p-mtg/mtgenv/experiments/stochastic_muzero
export PYTHONPATH=../../python
PY=./.venv/bin/python
echo "=== BACKFILL 3.2-heralds-combined-long (SUCCESS panel) ==="
$PY muzero_observability.py --config heralds_plain --ckpt-dir tb/3.2-muzero-heralds-combined-long/ckpt \
  --run-log heralds_combined_long.log --logdir tb/3.2-muzero-heralds-combined-long/log/serial \
  --run-name 3.2-muzero-heralds-combined-long --games 100 --sampled-games 40 --latent 256 --sims 50
echo "=== BACKFILL 3.0-muzero-swine (pure collapse) ==="
$PY muzero_observability.py --config swine_stoch --ckpt-dir tb/3.0-muzero-swine/ckpt \
  --run-log m3_run.log --logdir tb/3.0-muzero-swine/log/serial \
  --run-name 3.0-muzero-swine --games 100 --sampled-games 40 --latent 256 --sims 50
echo "=== BACKFILL 3.1-muzero-swine-shaped (weak-shaping collapse) ==="
$PY muzero_observability.py --config swine_stoch --ckpt-dir tb/3.1-muzero-swine-shaped/ckpt \
  --run-log m3_shaped.log --logdir tb/3.1-muzero-swine-shaped/log/serial \
  --run-name 3.1-muzero-swine-shaped --games 100 --sampled-games 40 --latent 256 --sims 50
echo "=== BACKFILL DONE ==="
