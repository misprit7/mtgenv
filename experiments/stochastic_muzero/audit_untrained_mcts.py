"""AUDIT — does an UNTRAINED Stochastic-MuZero MCTS systematically prefer the mulligan action?

The collapse mechanism hypothesis: MCTS collection loses ~100% from iteration one (log evidence:
collector reward_mean = -0.625 then -1.0), far below uniform-random's ~0.50. This probes the
starting point directly: build a *freshly initialized* (untrained) stochastic-muzero policy and, at
the very first real decision of a game (the Mulligan node, legal actions [96=mulligan, 97=keep]),
run its MCTS and report the chosen action + visit split + searched value — for several net inits.

If untrained MCTS piles visits on 96 (mulligan) across inits/sims, the cold-start collapse is an
MCTS/search-dynamics trap (deck-agnostic), not a reward-plumbing sign bug. Runs on CPU, fast.
"""
from __future__ import annotations

import argparse
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
    torch.manual_seed(seed)
    np.random.seed(seed)
    policy = create_policy(cfg.policy, model=None, enable_field=['learn', 'collect', 'eval'])
    return policy


def _flatten(obs_dict, keys):
    return np.concatenate([np.asarray(obs_dict[k], dtype=np.float32).ravel() for k in keys]).astype(np.float32)


def advance_to_mulligan(env, seed):
    """Reset and step (taking legal[0]) until we hit the Mulligan request; return obs/mask/info."""
    obs, info = env.reset(seed=seed)
    for _ in range(6):
        if str(info.get("request")) == "Mulligan":
            return obs, info
        mask = np.asarray(info["action_mask"], dtype=bool)
        # ChooseStartingPlayer etc — take first legal to progress
        obs, r, term, trunc, info = env.step(int(np.flatnonzero(mask)[0]))
        if term or trunc:
            break
    return obs, info


def probe(deck, net_seeds, sims, device):
    # Import the right config for the deck (dims differ).
    if deck == "heralds":
        from heralds_stochastic_muzero_config import main_config, create_config
    else:
        from swine_stochastic_muzero_config import main_config, create_config
    main_config.policy.num_simulations = sims

    env = MtgEnv(deck=deck, opponent="random")
    o0, _ = env.reset(seed=0)
    keys = list(o0.keys())

    print(f"\n===== UNTRAINED MCTS PROBE: deck={deck} sims={sims} =====")
    print("  (Mulligan node: legal=[96=MULLIGAN, 97=KEEP]; report visits on each + searched_value)")
    for ns in net_seeds:
        policy = build_untrained(main_config, create_config, device, ns)
        collect = policy.collect_mode
        evalm = policy.eval_mode
        # probe a few env states (all near-identical mulligan nodes)
        mull_choices_collect = []
        v96 = v97 = 0
        sval = []
        for es in range(6):
            obs, info = advance_to_mulligan(env, seed=1000 * ns + es)
            if str(info.get("request")) != "Mulligan":
                continue
            mask = np.asarray(info["action_mask"], dtype=np.int64)
            data = torch.from_numpy(_flatten(obs, keys)[None, :]).to(device)
            out_c = collect.forward(data, [mask], temperature=1.0, to_play=[-1])
            out_e = evalm.forward(data, [mask], [-1])
            dist = np.asarray(out_c[0]['visit_count_distributions'], dtype=np.float64)
            # distributions are over the LEGAL action set order = [96, 97]
            v96 += dist[0]; v97 += dist[1]
            sval.append(float(out_c[0]['searched_value']))
            mull_choices_collect.append(int(out_c[0]['action']))
            greedy = int(out_e[0]['action'])
        n = len(mull_choices_collect)
        pick96 = sum(1 for a in mull_choices_collect if a == 96)
        tot = v96 + v97
        print(f"  net_seed={ns}: collect picked MULLIGAN(96) {pick96}/{n} times | "
              f"visit-share 96={v96/tot:.2f} 97={v97/tot:.2f} | searched_value mean={np.mean(sval):+.3f} | "
              f"greedy(eval) last={greedy}")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="swine")
    ap.add_argument("--sims", type=int, default=50)
    ap.add_argument("--net-seeds", type=int, nargs="+", default=[0, 1, 2, 3, 4])
    ap.add_argument("--device", default="cpu")
    args = ap.parse_args()
    probe(args.deck, args.net_seeds, args.sims, args.device)
