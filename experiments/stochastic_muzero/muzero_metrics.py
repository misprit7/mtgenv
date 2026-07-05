"""METRIC PARITY (step 3) — log the gym's standard curves for a MuZero run so they overlay the PPO
baseline in one TensorBoard.

For a checkpoint (or every checkpoint in a run's ckpt/ dir) it computes, with GREEDY play:
  * `selfplay/winrate_vs_random`  — win % vs a random-legal opponent over N>=100 games (the exact tag
                                     PPO's SelfPlayEval logs; see python/selfplay_train.py:110).
  * `stats/productive_rate`       — productive_taken/productive_legal from info['decision_stats']
                                     (mirrors python/mtgenv_gym/tracked_stats.py's productive_rate).
and writes them to a torch TB SummaryWriter at the run dir (keyed by the checkpoint iteration), so a
still-collapsed run shows ~0 win / low productive honestly, right next to PPO's ~0.97.

NOTE on greedy + the low-index tie-break (see DEBUG AUDIT): a collapsed MuZero policy plays the
lowest-legal-index (PASS/mulligan) under argmax, so its greedy win-rate reads ~0 — that is the true,
faithful metric for a policy that hasn't learned, and is directly comparable to PPO's greedy eval.

Run (one ckpt):   PYTHONPATH=../../python .venv/bin/python muzero_metrics.py --config heralds_plain \
                      --model <run>/ckpt/ckpt_best.pth.tar --winrate-games 100
Run (whole run):  ... --ckpt-dir <run>/ckpt --logdir <run>   (evaluates every iteration_*.pth.tar)
"""
from __future__ import annotations

import argparse, glob, os, re
import numpy as np
import torch

import lz_patches  # noqa: F401
from ding.config import compile_config
from ding.policy import create_policy
from mtgenv_gym import MtgEnv


def _load_config(name):
    if name == "heralds_plain":
        import heralds_muzero_config as m
    elif name == "swine_plain":
        import swine_muzero_config as m
    elif name == "heralds_stoch":
        import heralds_stochastic_muzero_config as m
    else:
        import swine_stochastic_muzero_config as m
    return m.main_config, m.create_config


def build_policy(config_name, model_path, device, latent=None, sims=None):
    main_config, create_config = _load_config(config_name)
    if latent is not None:
        main_config.policy.model.latent_state_dim = int(latent)
    if sims is not None:
        main_config.policy.num_simulations = int(sims)
    cfg = compile_config(main_config, seed=0, env=None, auto=True, create_cfg=create_config, save_cfg=False)
    cfg.policy.device = device
    policy = create_policy(cfg.policy, model=None, enable_field=['learn', 'collect', 'eval'])
    policy.learn_mode.load_state_dict(torch.load(model_path, map_location=device))
    return policy, main_config


class Greedy:
    def __init__(self, policy, keys, device):
        self._eval = policy.eval_mode; self._keys = keys; self._device = device
    def _flat(self, o):
        return np.concatenate([np.asarray(o[k], dtype=np.float32).ravel() for k in self._keys]).astype(np.float32)
    def act(self, o, mask):
        data = torch.from_numpy(self._flat(o)[None, :]).to(self._device)
        return int(self._eval.forward(data, [np.asarray(mask, dtype=np.int64)], [-1])[0]['action'])


def eval_ckpt(agent, deck, games, seed0=5_000_000):
    """Greedy win-rate vs random-legal + productive_rate over `games` games (PPO's protocol)."""
    env = MtgEnv(deck=deck, opponent="random")
    wins = 0
    prod_num = prod_den = 0.0
    for i in range(games):
        obs, info = env.reset(seed=seed0 + i)
        done, r = False, 0.0
        while not done:
            a = agent.act(obs, info["action_mask"])
            obs, r, term, trunc, info = env.step(a)
            done = term or trunc
            st = info.get("decision_stats")
            if st:
                prod_num += float(st.get("productive_taken", 0.0))
                prod_den += float(st.get("productive_legal", 0.0))
        wins += 1 if r > 0.5 else 0
    wr = wins / games
    prod = (prod_num / prod_den) if prod_den > 0 else float("nan")
    return wr, prod


def _obs_keys(deck):
    e = MtgEnv(deck=deck); o, _ = e.reset(seed=0); return list(o.keys())


def _iter_from_name(path):
    m = re.search(r"iteration_(\d+)", os.path.basename(path))
    if m: return int(m.group(1))
    m = re.search(r"(\d+)", os.path.basename(path))
    return int(m.group(1)) if m else 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--config", default="heralds_plain", choices=["swine_stoch", "heralds_stoch", "heralds_plain"])
    ap.add_argument("--deck", default=None, help="defaults from config env.deck")
    ap.add_argument("--model", default=None, help="single checkpoint")
    ap.add_argument("--ckpt-dir", default=None, help="evaluate every iteration_*.pth.tar in this dir")
    ap.add_argument("--logdir", default=None, help="TB dir to write selfplay/winrate_vs_random + stats/productive_rate")
    ap.add_argument("--winrate-games", type=int, default=100)
    ap.add_argument("--latent", type=int, default=None)
    ap.add_argument("--sims", type=int, default=None)
    ap.add_argument("--device", default="cuda" if torch.cuda.is_available() else "cpu")
    args = ap.parse_args()

    main_config, _ = _load_config(args.config)
    deck = args.deck or main_config.env.deck
    keys = _obs_keys(deck)

    writer = None
    if args.logdir:
        from torch.utils.tensorboard import SummaryWriter
        writer = SummaryWriter(args.logdir)

    ckpts = ([args.model] if args.model else sorted(glob.glob(os.path.join(args.ckpt_dir, "*.pth.tar")),
                                                    key=_iter_from_name))
    if not ckpts or ckpts == [None]:
        raise SystemExit("give --model or --ckpt-dir")

    print(f"config={args.config} deck={deck} games={args.winrate_games} device={args.device}")
    for ck in ckpts:
        policy, _ = build_policy(args.config, ck, args.device, args.latent, args.sims)
        agent = Greedy(policy, keys, args.device)
        wr, prod = eval_ckpt(agent, deck, args.winrate_games)
        it = _iter_from_name(ck)
        print(f"  {os.path.basename(ck):28s} iter={it:6d}  winrate_vs_random={wr:.3f}  productive_rate={prod:.3f}")
        if writer:
            writer.add_scalar("selfplay/winrate_vs_random", wr, it)
            writer.add_scalar("stats/productive_rate", prod, it)
    if writer:
        writer.flush(); writer.close()


if __name__ == "__main__":
    main()
