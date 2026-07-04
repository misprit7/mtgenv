"""AUDIT — reproduce untrained-MCTS full-game play OUTSIDE the LightZero collector, to locate WHY
collection wins ~5% (vs uniform-random's ~51%) despite priority sampling looking ~uniform.

Drives full games vs a random opponent using the untrained stochastic-muzero policy in:
  * collect mode (sampled, temperature=0.25 — what the collector does), and
  * eval mode    (greedy argmax — what the evaluator/README's always-mulligan report does).

Records win-rate, episode length, and — per engine request type — the productive-action rate
(fraction of windows where a non-PASS/non-mulligan-to-death action was taken when available), so we
can see which decision class the untrained search botches vs uniform.
"""
from __future__ import annotations

import argparse
from collections import defaultdict
import numpy as np
import torch

import lz_patches  # noqa: F401
from ding.config import compile_config
from ding.policy import create_policy
from mtgenv_gym import MtgEnv


def build_untrained(main_config, create_config, device, seed):
    cfg = compile_config(main_config, seed=seed, env=None, auto=True,
                         create_cfg=create_config, save_cfg=False)
    cfg.policy.device = device
    torch.manual_seed(seed); np.random.seed(seed)
    return create_policy(cfg.policy, model=None, enable_field=['learn', 'collect', 'eval'])


def _flatten(obs_dict, keys):
    return np.concatenate([np.asarray(obs_dict[k], dtype=np.float32).ravel() for k in keys]).astype(np.float32)


def play(deck, mode, games, sims, device, net_seed=0):
    if deck == "heralds":
        from heralds_stochastic_muzero_config import main_config, create_config
    else:
        from swine_stochastic_muzero_config import main_config, create_config
    main_config.policy.num_simulations = sims
    policy = build_untrained(main_config, create_config, device, net_seed)
    fwd = policy.collect_mode if mode == "collect" else policy.eval_mode

    env = MtgEnv(deck=deck, opponent="random")
    o0, _ = env.reset(seed=0); keys = list(o0.keys())

    wins = 0
    lens = []
    # per-request: (# windows, # where action==0/PASS i.e. lowest index, # where action==mulligan(96))
    req_tot = defaultdict(int); req_pass = defaultdict(int)
    for g in range(games):
        obs, info = env.reset(seed=500_000 + g)
        done = False; steps = 0; r = 0.0
        while not done:
            mask = np.asarray(info["action_mask"], dtype=np.int64)
            legal = np.flatnonzero(mask)
            data = torch.from_numpy(_flatten(obs, keys)[None, :]).to(device)
            if mode == "collect":
                out = fwd.forward(data, [mask], temperature=0.25, to_play=[-1])
            else:
                out = fwd.forward(data, [mask], [-1])
            a = int(out[0]['action'])
            req = str(info.get("request"))
            req_tot[req] += 1
            if a == int(legal[0]):   # lowest-index legal (PASS at priority; mulligan at Mulligan)
                req_pass[req] += 1
            obs, r, term, trunc, info = env.step(a)
            done = term or trunc; steps += 1
        if r > 0.5:
            wins += 1
        lens.append(steps)
    lens = np.array(lens)
    print(f"\n== deck={deck} mode={mode} net_seed={net_seed} games={games} sims={sims} ==")
    print(f"   win-rate={wins/games:.3f}  len mean={lens.mean():.1f} median={np.median(lens):.0f} "
          f"min={lens.min()} max={lens.max()}")
    print(f"   per-request lowest-index(PASS/mull) selection rate:")
    for req in sorted(req_tot, key=lambda k: -req_tot[k]):
        tot = req_tot[req]
        print(f"       {req:22s} windows={tot:5d}  lowest-idx-rate={req_pass[req]/tot:.2f}")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="swine")
    ap.add_argument("--games", type=int, default=40)
    ap.add_argument("--sims", type=int, default=50)
    ap.add_argument("--device", default="cuda")
    ap.add_argument("--net-seed", type=int, default=0)
    args = ap.parse_args()
    for mode in ("collect", "eval"):
        play(args.deck, mode, args.games, args.sims, args.device, args.net_seed)
