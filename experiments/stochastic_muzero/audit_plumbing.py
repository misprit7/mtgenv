"""PLUMBING AUDIT (step 1) — does the swine LightZero adapter faithfully surface ±1 terminal
reward, from the learner's perspective, at the final step, with a ~0.535 random-seat win-rate?

Runs the *real adapter* (`MtgSwineEnv`, shaping OFF) with a uniform-random-over-legal policy —
no MCTS, no LightZero learner — and checks the invariants the lead flagged:

  (a) learner-seat win-rate ≈ 0.535 over N episodes,
  (b) `eval_episode_return` (and terminal step reward) is ±1, ~half positive, on the FINAL step,
  (c) NO reward anywhere except the terminal step (pure env, shaping off),
  (d) episode lengths sane.

Run:  PYTHONPATH=../../python .venv/bin/python audit_plumbing.py --episodes 300 --deck swine
"""
from __future__ import annotations

import argparse
import numpy as np

from swine_lightzero_env import MtgSwineEnv


def rollout_audit(deck: str, episodes: int, seed: int = 12345):
    rng = np.random.default_rng(seed)
    cfg = MtgSwineEnv.default_config()
    cfg.deck = deck
    cfg.reward_shaping = 0.0  # pure sparse env — the mode the audit is about
    env = MtgSwineEnv(cfg)
    env.seed(seed, dynamic_seed=True)

    wins = losses = draws_or_trunc = 0
    ep_lens = []
    final_rewards = []
    # invariant trackers
    nonterminal_nonzero = 0          # (c): reward != 0 on a non-terminal step
    terminal_not_pm1 = 0             # (b): terminal reward not in {-1,0,+1}
    eval_return_mismatch = 0         # eval_episode_return != final step reward
    reward_only_at_terminal = True

    for ep in range(episodes):
        obs = env.reset()
        done = False
        steps = 0
        ep_reward_trace = []
        last_reward = 0.0
        eval_return = None
        while not done:
            legal = np.nonzero(obs['action_mask'])[0]
            assert legal.size >= 1, "empty mask surfaced to learner"
            a = int(rng.choice(legal))
            ts = env.step(a)
            obs = ts.obs
            r = float(np.asarray(ts.reward))
            done = bool(ts.done)
            steps += 1
            ep_reward_trace.append(r)
            last_reward = r
            if not done and abs(r) > 1e-9:
                nonterminal_nonzero += 1
                reward_only_at_terminal = False
            if done:
                eval_return = float(ts.info.get('eval_episode_return', np.nan))
                if r not in (-1.0, 0.0, 1.0):
                    terminal_not_pm1 += 1
                if not (np.isnan(eval_return) or abs(eval_return - r) < 1e-6):
                    eval_return_mismatch += 1

        ep_lens.append(steps)
        final_rewards.append(last_reward)
        if last_reward > 0.5:
            wins += 1
        elif last_reward < -0.5:
            losses += 1
        else:
            draws_or_trunc += 1

    ep_lens = np.array(ep_lens)
    final_rewards = np.array(final_rewards)
    print(f"\n===== PLUMBING AUDIT: deck={deck}, episodes={episodes} =====")
    print(f"(a) learner-seat win-rate : {wins/episodes:.3f}   (wins={wins} losses={losses} draw/trunc={draws_or_trunc})")
    print(f"    expected random-seat baseline ~0.535")
    print(f"(b) terminal reward values : unique={sorted(set(final_rewards.tolist()))}")
    print(f"    frac positive (+1)     : {(final_rewards > 0.5).mean():.3f}   frac negative (-1): {(final_rewards < -0.5).mean():.3f}")
    print(f"    terminal reward NOT in {{-1,0,+1}} count: {terminal_not_pm1}")
    print(f"    eval_episode_return != final-step reward count: {eval_return_mismatch}")
    print(f"(c) non-terminal nonzero reward count: {nonterminal_nonzero}   (reward ONLY at terminal: {reward_only_at_terminal})")
    print(f"(d) episode length (sub-decisions): mean={ep_lens.mean():.1f} median={np.median(ep_lens):.0f} "
          f"min={ep_lens.min()} max={ep_lens.max()} p95={np.percentile(ep_lens,95):.0f}")
    trunc = draws_or_trunc  # a truncation/real-draw both land here; break out below
    print(f"    (zero-reward terminals — draws or truncations — : {draws_or_trunc})")
    return dict(win_rate=wins/episodes, ep_lens=ep_lens, final_rewards=final_rewards)


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--episodes", type=int, default=300)
    ap.add_argument("--deck", default="swine")
    ap.add_argument("--seed", type=int, default=12345)
    args = ap.parse_args()
    rollout_audit(args.deck, args.episodes, args.seed)
