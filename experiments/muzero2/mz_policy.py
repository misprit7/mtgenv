"""evalkit ``Policy`` adapter for LightZero MuZero / Gumbel-MuZero checkpoints + an eval CLI/watcher.

The mandate: report ALL numbers through evalkit (``evaluate_checkpoint``) — greedy AND sampled, Wilson
CIs, productive/attack rates, the swine chump/gang analyzer — overlaid with the PPO baseline on the
shared TB board. This module is the single integration point:

  * ``MuZeroLzPolicy(BasePolicy)`` — batched ``act`` running LightZero's C++ MCTS over a whole round of
    roots, returning a fair-greedy action (argmax visit counts, ties broken by the network PRIOR — NOT
    lowest index, which would make a weak net read ~0 by always PASSing) or a visits^(1/temp) sample.
  * ``build_policy`` — construct the policy from mtg_config.build_configs and load a checkpoint.
  * CLI — eval one checkpoint or a whole ckpt/ dir into a shared-board run via evaluate_checkpoint,
    with iter->env_step recovered from the training log so the x-axis matches PPO.
  * ``--watch`` — poll a run's ckpt/ dir and eval each new checkpoint as training produces it.

Run (one ckpt):   PYTHONPATH=../../python .venv/bin/python mz_policy.py --algo gumbel --deck heralds \
                      --ckpt <run>/ckpt/ckpt_best.pth.tar --step 100000 --run-dir /tmp/mtgenv_tb/3.4-gumbel-heralds
Run (watch):      ... --algo gumbel --deck heralds --run-dir /tmp/mtgenv_tb/3.4-gumbel-heralds \
                      --run-log 3.4.log --watch
"""
from __future__ import annotations

import argparse
import glob
import os
import re
import time

import numpy as np
import torch

from mtgenv_gym.evalkit import BasePolicy


# ──────────────────────────────────────────────────────────────────────────────────────────────
# Policy construction
# ──────────────────────────────────────────────────────────────────────────────────────────────
def build_policy(algo: str, deck: str, ckpt_path: str, device: str = "cuda",
                 latent: int = 512, sims: "int | None" = None, head_hidden=(64,)):
    """Build a LightZero policy matching the trained config and load ``ckpt_path`` into it."""
    from ding.config import compile_config
    from ding.policy import create_policy
    from mtg_config import build_configs

    kw = dict(algo=algo, deck=deck, exp_name="eval_tmp", latent_state_dim=int(latent),
              head_hidden=tuple(head_hidden), cuda=(device == "cuda"))
    if sims is not None:
        kw["num_simulations"] = int(sims)
    main_config, create_config = build_configs(**kw)
    cfg = compile_config(main_config, seed=0, env=None, auto=True, create_cfg=create_config,
                         save_cfg=False)
    cfg.policy.device = device
    policy = create_policy(cfg.policy, model=None, enable_field=['learn', 'collect', 'eval'])
    policy.learn_mode.load_state_dict(torch.load(ckpt_path, map_location=device))
    return policy


class MuZeroLzPolicy(BasePolicy):
    """Batched evalkit adapter over a LightZero MuZero/Gumbel policy's eval-mode MCTS.

    Stateless across ``act`` calls (each MCTS forward builds fresh roots), so ``env_indices`` is ignored.

    **Algo-aware action selection (critical).** Gumbel-MuZero's eval action is NOT argmax visit counts —
    its ``_forward_eval`` returns ``action = argmax(improved_policy_probs)`` (completed-Q improved policy),
    which at low sims routinely differs from argmax visits (e.g. it picks a 3-visit action over a
    22-visit one). Re-deriving from visit counts inverts Gumbel's decisions → a passive/losing eval even
    when collection wins. So:
      * ``gumbel``: greedy = the framework's ``action``; sample = softmax(policy-head logits) — the head
        is trained toward the improved policy, so it is the honest stochastic learning-signal curve.
      * ``muzero``: greedy = fair-greedy (argmax visits, ties broken by the network prior, not lowest
        index); sample = visits^(1/temp). (Plain MuZero's visit counts DO reflect its policy.)"""

    def __init__(self, policy, device: str = "cuda", temp: float = 0.25, algo: str = "gumbel"):
        self._eval = policy.eval_mode
        self._model = getattr(policy, "_eval_model", None) or getattr(policy, "_learn_model")
        self._device = device
        self._temp = float(temp)
        self._algo = algo
        self._keys = None  # obs concat order, captured from the first obs (stable across the run)

    def _flatten_batch(self, obs_batch):
        if self._keys is None:
            self._keys = list(obs_batch[0].keys())
        rows = [np.concatenate([np.asarray(o[k], dtype=np.float32).ravel() for k in self._keys])
                for o in obs_batch]
        return torch.from_numpy(np.stack(rows).astype(np.float32)).to(self._device)

    def act(self, obs_batch, mask_batch, *, mode="greedy", env_indices=None):
        B = len(obs_batch)
        data = self._flatten_batch(obs_batch)
        masks_i = [np.asarray(m, dtype=np.int64) for m in mask_batch]
        out = self._eval.forward(data, masks_i, [-1] * B)

        # network policy-head prior over the full action space — one batched forward (used for the muzero
        # greedy tie-break and the gumbel sampled policy).
        with torch.no_grad():
            logits = self._model.initial_inference(data).policy_logits.detach().cpu().numpy()

        acts = np.empty(B, dtype=np.int64)
        for i in range(B):
            legal = np.flatnonzero(masks_i[i])
            if self._algo == "gumbel":
                if mode == "greedy":
                    acts[i] = int(out[i]['action'])          # framework improved-policy argmax
                else:
                    lg = logits[i][legal]
                    p = np.exp((lg - lg.max()) / max(self._temp, 1e-6)); p /= p.sum()
                    acts[i] = int(legal[int(np.random.choice(len(legal), p=p))])
            else:  # muzero — visit counts reflect the policy
                dist = np.asarray(out[i]['visit_count_distributions'], dtype=np.float64)  # over legal, in-order
                if mode == "greedy":
                    lg = logits[i][legal]
                    prior = np.exp(lg - lg.max()); prior /= prior.sum()
                    acts[i] = int(legal[int(np.argmax(dist + 1e-6 * prior))])  # visits dominate; prior breaks ties
                else:
                    p = np.power(np.maximum(dist, 0.0), 1.0 / self._temp)
                    p = np.ones_like(p) if p.sum() <= 0 else p / p.sum()
                    acts[i] = int(legal[int(np.random.choice(len(legal), p=p))])
        return acts


# ──────────────────────────────────────────────────────────────────────────────────────────────
# env-step recovery + eval driver
# ──────────────────────────────────────────────────────────────────────────────────────────────
def ckpt_envstep_map(run_log):
    """Parse a LightZero stdout log → {train_iter -> env_step} using the latest total_envstep_count
    seen before each 'iteration_N.pth.tar' save (iteration_0 -> 0)."""
    m = {0: 0}
    if not run_log or not os.path.exists(run_log):
        return m
    last = 0
    for line in open(run_log, errors="ignore"):
        s = re.search(r"total_envstep_count[':\s]+(\d+)", line)
        if s:
            last = int(s.group(1)); continue
        c = re.search(r"iteration_(\d+)\.pth\.tar", line)
        if c:
            m[int(c.group(1))] = last
    return m


def _iter_of(path):
    mm = re.search(r"iteration_(\d+)", os.path.basename(path))
    return int(mm.group(1)) if mm else None


def eval_one(algo, deck, ckpt, step, run_dir, run_name, device, games, latent, sims, temp,
             writer=None, head=(64,)):
    from mtgenv_gym.evalkit import evaluate_checkpoint
    policy = build_policy(algo, deck, ckpt, device=device, latent=latent, sims=sims, head_hidden=head)
    adapter = MuZeroLzPolicy(policy, device=device, temp=temp, algo=algo)
    res = evaluate_checkpoint(adapter, step=int(step), run_dir=run_dir, deck=deck, games=games,
                              run_name=run_name, algo=f"{algo}-mz", writer=writer)
    r = res["selfplay/winrate_vs_random"]
    g, s = r["greedy"], r["sample"]
    extra = ""
    if g.analyzers:
        extra = "  " + " ".join(f"{k.split('/')[-1]}={v:.2f}" for k, v in g.analyzers.items())
    print(f"  step={step:>8} greedy={g.win_rate:.3f} (95% {g.win_ci95[0]:.2f}-{g.win_ci95[1]:.2f}) "
          f"sampled={s.win_rate:.3f}  prod={g.stats.get('productive_rate', float('nan')):.2f} "
          f"atk={g.stats.get('attack_rate', float('nan')):.2f} turns={g.avg_turns:.1f}{extra}", flush=True)
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--algo", required=True, choices=["gumbel", "muzero"])
    ap.add_argument("--deck", required=True)
    ap.add_argument("--run-dir", required=True, help="shared-board TB run dir (also where JSON/replay go)")
    ap.add_argument("--run-name", default=None)
    ap.add_argument("--ckpt", default=None, help="single checkpoint")
    ap.add_argument("--ckpt-dir", default=None, help="eval every iteration_*.pth.tar (+ ckpt_best) here")
    ap.add_argument("--run-log", default=None, help="training stdout log for iter->env_step mapping")
    ap.add_argument("--step", type=int, default=0, help="env-step for a single --ckpt (if no --run-log)")
    ap.add_argument("--games", type=int, default=100)
    ap.add_argument("--latent", type=int, default=512)
    ap.add_argument("--head", type=int, default=64, help="head hidden width (match training)")
    ap.add_argument("--sims", type=int, default=None)
    ap.add_argument("--temp", type=float, default=0.25)
    ap.add_argument("--device", default="cuda" if torch.cuda.is_available() else "cpu")
    ap.add_argument("--watch", action="store_true", help="poll ckpt-dir and eval new checkpoints forever")
    ap.add_argument("--poll", type=int, default=180, help="watch poll interval (s)")
    args = ap.parse_args()

    run_name = args.run_name or os.path.basename(args.run_dir.rstrip("/"))
    from torch.utils.tensorboard import SummaryWriter
    writer = SummaryWriter(args.run_dir)

    def step_for(ck):
        it = _iter_of(ck)
        if args.run_log:
            return ckpt_envstep_map(args.run_log).get(it, it or args.step)
        return it if it is not None else args.step

    if args.ckpt:
        eval_one(args.algo, args.deck, args.ckpt, args.step, args.run_dir, run_name, args.device,
                 args.games, args.latent, args.sims, args.temp, writer=writer, head=(args.head,))
    elif args.watch:
        seen = set()
        print(f"[watch] polling {args.ckpt_dir} every {args.poll}s → {args.run_dir}", flush=True)
        while True:
            cks = sorted(glob.glob(os.path.join(args.ckpt_dir, "iteration_*.pth.tar")),
                         key=lambda p: _iter_of(p) or 0)
            for ck in cks:
                if ck in seen:
                    continue
                try:
                    eval_one(args.algo, args.deck, ck, step_for(ck), args.run_dir, run_name,
                             args.device, args.games, args.latent, args.sims, args.temp, writer=writer, head=(args.head,))
                    seen.add(ck)
                except Exception as e:
                    print(f"  [watch] {os.path.basename(ck)} failed: {type(e).__name__}: {e}", flush=True)
                    seen.add(ck)
            writer.flush()
            time.sleep(args.poll)
    else:
        cks = sorted(glob.glob(os.path.join(args.ckpt_dir, "iteration_*.pth.tar")),
                     key=lambda p: _iter_of(p) or 0)
        best = os.path.join(args.ckpt_dir, "ckpt_best.pth.tar")
        for ck in cks:
            eval_one(args.algo, args.deck, ck, step_for(ck), args.run_dir, run_name, args.device,
                     args.games, args.latent, args.sims, args.temp, writer=writer, head=(args.head,))
        if os.path.exists(best):
            mx = max((step_for(c) for c in cks), default=0)
            eval_one(args.algo, args.deck, best, mx, args.run_dir, run_name, args.device,
                     args.games, args.latent, args.sims, args.temp, writer=writer, head=(args.head,))
    writer.flush(); writer.close()


if __name__ == "__main__":
    main()
