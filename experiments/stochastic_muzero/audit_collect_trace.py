"""AUDIT — instrument the REAL LightZero collector to see HOW collected games are lost.

Monkeypatches ``MtgSwineEnv`` to record, per completed episode: length (sub-decisions), the number
of MULLIGAN(96) actions the collector took, and the terminal reward. Then runs ``train_muzero`` for
a short budget so we watch the ACTUAL collection (real net, real temperature, real MCTS) rather than
theorize. Dumps a per-episode table + summary at the end.

Answers: is the ~100% collection loss (log: reward_mean -> -1.0) driven by mulligan-to-death, or by
losing normal-length games? And does it start already-collapsed (iter 0, untrained) or degrade?

Run:  PYTHONPATH=../../python .venv/bin/python audit_collect_trace.py --deck swine --max-steps 4000
"""
from __future__ import annotations

import argparse
import sys
import numpy as np

import lz_patches  # noqa: F401
import swine_lightzero_env as SLE

# Global episode recorder (collected across all collector envs).
EPISODES = []  # list of dict(len, mull, reward, deck)
# Per-request lowest-index (PASS at priority / mulligan) selection tally during the REAL collect.
from collections import defaultdict
REQ_TOT = defaultdict(int)
REQ_LOW = defaultdict(int)


def _install_recorder():
    Env = SLE.MtgSwineEnv
    _orig_reset = Env.reset
    _orig_step = Env.step

    def reset(self):
        self._trace_mull = 0
        self._trace_len = 0
        return _orig_reset(self)

    def step(self, action):
        # self._mask is the mask for THIS decision (set by the previous reset/step's _lz_obs).
        m = np.asarray(self._mask, dtype=bool)
        legal = np.flatnonzero(m)
        if legal.size:
            bucket = f"n_legal>={min(legal.size,6)}" if legal.size < 6 else "n_legal>=6"
            REQ_TOT[bucket] += 1
            if int(action) == int(legal[0]):
                REQ_LOW[bucket] += 1
        if int(action) == 96:
            self._trace_mull = getattr(self, "_trace_mull", 0) + 1
        self._trace_len = getattr(self, "_trace_len", 0) + 1
        ts = _orig_step(self, action)
        if ts.done:
            r = float(np.asarray(ts.reward))
            # eval_episode_return is the RAW ±1 outcome (shaping-independent)
            outcome = float(ts.info.get("eval_episode_return", r))
            EPISODES.append(dict(len=self._trace_len, mull=self._trace_mull,
                                 reward=outcome, deck=self._deck))
        return ts

    Env.reset = reset
    Env.step = step


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="swine")
    ap.add_argument("--max-steps", type=int, default=4000)
    args, _ = ap.parse_known_args()

    _install_recorder()

    # Force a tiny/quick real-ish run: small latent so learning is cheap, but real sims/temperature.
    sys.argv = [sys.argv[0]]  # clear flags so configs don't see --deck etc.
    if args.deck == "heralds":
        from heralds_stochastic_muzero_config import main_config, create_config
    else:
        from swine_stochastic_muzero_config import main_config, create_config

    main_config.exp_name = f"tb/_audit_collect_trace_{args.deck}"
    main_config.policy.model.latent_state_dim = 128
    main_config.policy.update_per_collect = 20   # keep learning cheap so we see many collects fast
    main_config.env.collector_env_num = 8
    main_config.env.n_evaluator_episode = 2
    main_config.env.evaluator_env_num = 2
    main_config.policy.n_episode = 8
    main_config.policy.eval_freq = int(1e9)      # skip eval entirely for speed

    from lzero.entry import train_muzero
    try:
        train_muzero([main_config, create_config], seed=0, max_env_step=args.max_steps)
    except Exception as e:
        print("train stopped:", type(e).__name__, str(e)[:200])

    ep = EPISODES
    print(f"\n===== COLLECT TRACE: deck={args.deck}, {len(ep)} completed episodes =====")
    if not ep:
        print("  (no completed episodes)")
        return
    arr_len = np.array([e['len'] for e in ep])
    arr_mull = np.array([e['mull'] for e in ep])
    arr_rew = np.array([e['reward'] for e in ep])
    n = len(ep)
    # A "mulligan-to-death" heuristic: >=5 mulligans taken and a short game.
    mull_death = (arr_mull >= 5)
    print(f"  win-rate (mean reward>0)      : {(arr_rew > 0.5).mean():.3f}   reward_mean={arr_rew.mean():+.3f}")
    print(f"  episode length                : mean={arr_len.mean():.1f} median={np.median(arr_len):.0f} min={arr_len.min()} max={arr_len.max()}")
    print(f"  mulligans(96) taken per game  : mean={arr_mull.mean():.2f} median={np.median(arr_mull):.0f} max={arr_mull.max()}")
    print(f"  games with >=5 mulligans      : {mull_death.sum()}/{n} ({mull_death.mean():.2f})")
    print(f"  win-rate | mull<5             : {(arr_rew[arr_mull<5] > 0.5).mean() if (arr_mull<5).any() else float('nan'):.3f}  (n={(arr_mull<5).sum()})")
    print(f"  win-rate | mull>=5            : {(arr_rew[arr_mull>=5] > 0.5).mean() if (arr_mull>=5).any() else float('nan'):.3f}  (n={(arr_mull>=5).sum()})")
    # first vs last third to see degradation over collects
    third = max(1, n // 3)
    print(f"  FIRST {third} eps reward_mean : {arr_rew[:third].mean():+.3f}  mull_mean={arr_mull[:third].mean():.2f}")
    print(f"  LAST  {third} eps reward_mean : {arr_rew[-third:].mean():+.3f}  mull_mean={arr_mull[-third:].mean():.2f}")
    print(f"  --- REAL-collector lowest-index (PASS/mull) selection rate by legal-count bucket ---")
    for b in sorted(REQ_TOT):
        print(f"      {b:12s} windows={REQ_TOT[b]:6d}  lowest-idx-rate={REQ_LOW[b]/REQ_TOT[b]:.2f}")


if __name__ == "__main__":
    main()
