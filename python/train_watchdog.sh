#!/usr/bin/env bash
# Resilient overnight training (gym). Runs export_replays self-play; if the run FREEZES — tfevents
# stops growing while the process is still alive, i.e. an in-engine infinite loop that the Python
# max_decisions cap can't catch (task #55) — or exits non-zero, it kills and RESTARTS the run. This
# keeps training continuously alive through the night despite the known engine loop bug, until the
# engine's hard iteration ceiling lands (then this is just belt-and-suspenders). Each restart is a
# fresh run (pool cleaned), so curves split across tensorboard runs <RUN>_1, _2, … — that's expected.
#
#   nohup python/train_watchdog.sh selesnya 5000000 selesnya-cardid-5M >/tmp/mtgenv_wd_main.log 2>&1 &
set -u
DECK="${1:-selesnya}"
STEPS="${2:-5000000}"
RUN="${3:-${DECK}-cardid}"
STALE_S="${4:-200}"          # consider frozen if newest tfevents hasn't grown in this many seconds
TB=/tmp/mtgenv_tb
POOL="/tmp/mtgenv_pool_${DECK}_wd"
LOG=/tmp/mtgenv_wd.log
cd /home/xander/dev/p-mtg/mtgenv || exit 1
export PYTHONPATH=python PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1

stamp() { date +%H:%M:%S; }
echo "$(stamp) watchdog start: deck=$DECK steps=$STEPS run=$RUN stale=${STALE_S}s" >>"$LOG"

while true; do
  python/.venv/bin/python -c "import shutil; shutil.rmtree('$POOL', ignore_errors=True); shutil.rmtree('${POOL}_ladder', ignore_errors=True)"
  python/.venv/bin/python python/export_replays.py --deck "$DECK" --timesteps "$STEPS" \
      --record-every 500000 --n-envs 32 --tensorboard "$TB" --pool-dir "$POOL" --run-name "$RUN" \
      >>"/tmp/mtgenv_${DECK}_wd.log" 2>&1 &
  PID=$!
  echo "$(stamp) launched $RUN pid=$PID" >>"$LOG"

  # progress monitor: watch the newest tfevents mtime for this run
  while kill -0 "$PID" 2>/dev/null; do
    sleep 60
    f=$(ls -t "$TB/${RUN}"_*/events* 2>/dev/null | head -1)
    if [ -n "$f" ]; then
      age=$(( $(date +%s) - $(stat -c %Y "$f" 2>/dev/null || echo 0) ))
      if [ "$age" -gt "$STALE_S" ]; then
        echo "$(stamp) FROZEN: $RUN tfevents ${age}s stale — killing pid=$PID" >>"$LOG"
        kill -9 "$PID" 2>/dev/null
        break
      fi
    fi
  done

  wait "$PID" 2>/dev/null; rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "$(stamp) $RUN COMPLETED ok — watchdog exiting" >>"$LOG"
    break
  fi
  echo "$(stamp) $RUN ended rc=$rc (frozen/crashed) — restarting in 5s" >>"$LOG"
  sleep 5
done
