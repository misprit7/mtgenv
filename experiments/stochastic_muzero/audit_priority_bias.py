"""AUDIT — at a normal PRIORITY window, does untrained MCTS collection over-select PASS / low
indices vs uniform-random?  (Explains collection win ~5% << random's ~51% despite normal-length,
mostly-no-mulligan games.)

Walk a fresh game to its first Priority decision that has >=4 legal actions, then call the collect
policy's MCTS many times (fresh Dirichlet noise each) and tally the sampled action + the mean visit
distribution. Compare PASS(action 0)'s selection share to uniform (1/n_legal). Also report the
greedy(eval) action. Repeat for a few net inits and a few game states.
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
    torch.manual_seed(seed); np.random.seed(seed)
    return create_policy(cfg.policy, model=None, enable_field=['learn', 'collect', 'eval'])


def _flatten(obs_dict, keys):
    return np.concatenate([np.asarray(obs_dict[k], dtype=np.float32).ravel() for k in keys]).astype(np.float32)


def walk_to_priority(env, seed, min_legal=4, rng=None):
    obs, info = env.reset(seed=seed)
    for _ in range(40):
        mask = np.asarray(info["action_mask"], dtype=bool)
        legal = np.flatnonzero(mask)
        if str(info.get("request")) == "Priority" and legal.size >= min_legal and 0 in legal:
            return obs, info
        a = int(rng.choice(legal)) if rng is not None else int(legal[0])
        obs, r, term, trunc, info = env.step(a)
        if term or trunc:
            return None, None
    return None, None


def probe(deck, net_seeds, sims, samples, device):
    if deck == "heralds":
        from heralds_stochastic_muzero_config import main_config, create_config
    else:
        from swine_stochastic_muzero_config import main_config, create_config
    main_config.policy.num_simulations = sims

    env = MtgEnv(deck=deck, opponent="random")
    o0, _ = env.reset(seed=0); keys = list(o0.keys())
    rng = np.random.default_rng(0)

    print(f"\n===== PRIORITY-NODE BIAS PROBE: deck={deck} sims={sims} samples={samples} =====")
    print("  collect temperature=0.25 (the real collector value). PASS=action 0 (lowest index).")
    for ns in net_seeds:
        policy = build_untrained(main_config, create_config, device, ns)
        collect, evalm = policy.collect_mode, policy.eval_mode
        obs, info = walk_to_priority(env, seed=10 * ns + 1, min_legal=4, rng=rng)
        if obs is None:
            print(f"  net_seed={ns}: (no suitable priority node found)"); continue
        mask = np.asarray(info["action_mask"], dtype=np.int64)
        legal = np.flatnonzero(mask)
        n_legal = legal.size
        data = torch.from_numpy(_flatten(obs, keys)[None, :]).to(device)
        picks = []
        mean_dist = np.zeros(n_legal)
        for _ in range(samples):
            out = collect.forward(data, [mask], temperature=0.25, to_play=[-1])
            picks.append(int(out[0]['action']))
            mean_dist += np.asarray(out[0]['visit_count_distributions'], dtype=np.float64)
        mean_dist /= samples
        picks = np.array(picks)
        pass_share = float((picks == 0).mean())
        greedy = int(evalm.forward(data, [mask], [-1])[0]['action'])
        # visit share on the lowest-index legal action (== PASS here since 0 in legal)
        low_idx_visit = mean_dist[0] / mean_dist.sum()
        print(f"  net_seed={ns}: n_legal={n_legal} legal={legal.tolist()[:8]} | "
              f"PASS(0) sampled-share={pass_share:.2f} (uniform={1/n_legal:.2f}) | "
              f"PASS mean-visit-share={low_idx_visit:.2f} | greedy={greedy}")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="swine")
    ap.add_argument("--sims", type=int, default=50)
    ap.add_argument("--samples", type=int, default=40)
    ap.add_argument("--net-seeds", type=int, nargs="+", default=[0, 1, 2, 3, 4])
    ap.add_argument("--device", default="cuda")
    args = ap.parse_args()
    probe(args.deck, args.net_seeds, args.sims, args.samples, args.device)
