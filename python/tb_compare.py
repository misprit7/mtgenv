"""Pull end-of-training values + trajectory for chosen TB scalars across runs (quick comparison)."""
import glob, os, sys
from tensorboard.backend.event_processing.event_accumulator import EventAccumulator

TAGS = ["stats/block_rate", "stats/block_double_rate", "stats/productive_rate",
        "stats/attack_rate", "selfplay/winrate_vs_random", "selfplay/winrate_vs_initial"]


def load(run_dir):
    subs = sorted(glob.glob(os.path.join(run_dir, "**", "events.out.tfevents.*"), recursive=True))
    if not subs:
        return None
    ea = EventAccumulator(os.path.dirname(subs[-1]), size_guidance={"scalars": 0}); ea.Reload()
    avail = set(ea.Tags().get("scalars", []))
    out = {}
    for t in TAGS:
        if t in avail:
            xs = [(s.step, s.value) for s in ea.Scalars(t)]
            out[t] = xs
    return out, sorted(avail)


for run in sys.argv[1:]:
    name = os.path.basename(run.rstrip("/"))
    res = load(run)
    if res is None:
        print(f"\n### {name}: (no events)"); continue
    scalars, avail = res
    print(f"\n### {name}")
    for t in TAGS:
        if t in scalars and scalars[t]:
            xs = scalars[t]
            first = next((v for _s, v in xs if v == v), float("nan"))
            last = xs[-1][1]
            # midpoint value too (trajectory shape)
            mid = xs[len(xs) // 2][1]
            print(f"  {t:34s} first={first:6.3f}  mid={mid:6.3f}  last={last:6.3f}  (n={len(xs)})")
        else:
            print(f"  {t:34s} (absent)")
