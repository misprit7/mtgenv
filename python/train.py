"""Milestone-1 training: MaskablePPO on ``MtgEnv`` (agent_seat=0 vs a random opponent), then
report win-rate vs random — the M1 exit criterion (GYM_PLAN §8.1).

    PYTHONPATH=python python python/train.py --deck demo --timesteps 60000 --eval-games 400

``train_and_eval`` is importable so the pytest learning-sanity test can run a short version and
assert the win-rate climbs above the ~50% a random agent gets.
"""

from __future__ import annotations

import argparse

import numpy as np
from sb3_contrib import MaskablePPO
from sb3_contrib.common.wrappers import ActionMasker
from stable_baselines3.common.vec_env import DummyVecEnv

from mtgenv_gym import MtgEnv
from mtgenv_gym.policy import EntityExtractor


def _mask_fn(env):
    # ActionMasker calls this as `fn(self.env)`; `env` is the inner MtgEnv.
    return env.action_masks()


def _make_env(deck, auto_pass, seed):
    def thunk():
        env = MtgEnv(deck=deck, auto_pass=auto_pass)
        env = ActionMasker(env, _mask_fn)
        env.reset(seed=seed)
        return env

    return thunk


def make_model(deck="demo", auto_pass=True, n_envs=8, seed=0, **ppo_kwargs):
    venv = DummyVecEnv([_make_env(deck, auto_pass, seed + i) for i in range(n_envs)])
    policy_kwargs = dict(features_extractor_class=EntityExtractor)
    defaults = dict(n_steps=256, batch_size=256, gamma=0.999, ent_coef=0.01, verbose=0, seed=seed)
    defaults.update(ppo_kwargs)
    return MaskablePPO("MultiInputPolicy", venv, policy_kwargs=policy_kwargs, **defaults)


def evaluate(model, deck="demo", auto_pass=True, n_games=400, seed0=1_000_000):
    """Greedy win-rate for the trained policy (agent_seat=0) vs a random opponent."""
    env = MtgEnv(deck=deck, auto_pass=auto_pass)
    wins = draws = losses = 0
    for i in range(n_games):
        obs, info = env.reset(seed=seed0 + i)
        done = False
        reward = 0.0
        while not done:
            action, _ = model.predict(obs, action_masks=info["action_mask"], deterministic=True)
            obs, reward, term, trunc, info = env.step(int(action))
            done = term or trunc
        if reward > 0:
            wins += 1
        elif reward < 0:
            losses += 1
        else:
            draws += 1
    return wins, draws, losses


def random_baseline(deck="demo", auto_pass=True, n_games=400, seed0=2_000_000):
    """Win-rate of a *random* agent_seat vs a random opponent — the bar to beat."""
    env = MtgEnv(deck=deck, auto_pass=auto_pass)
    rng = np.random.default_rng(0)
    wins = draws = losses = 0
    for i in range(n_games):
        obs, info = env.reset(seed=seed0 + i)
        done = False
        reward = 0.0
        while not done:
            legal = np.flatnonzero(info["action_mask"])
            obs, reward, term, trunc, info = env.step(int(rng.choice(legal)))
            done = term or trunc
        if reward > 0:
            wins += 1
        elif reward < 0:
            losses += 1
        else:
            draws += 1
    return wins, draws, losses


def train_and_eval(deck="demo", timesteps=60_000, eval_games=400, n_envs=8, seed=0):
    model = make_model(deck=deck, n_envs=n_envs, seed=seed)
    model.learn(total_timesteps=timesteps, progress_bar=False)
    w, d, l = evaluate(model, deck=deck, n_games=eval_games)
    bw, bd, bl = random_baseline(deck=deck, n_games=eval_games)
    return {
        "trained": {"win": w, "draw": d, "loss": l, "win_rate": w / eval_games},
        "baseline": {"win": bw, "draw": bd, "loss": bl, "win_rate": bw / eval_games},
        "model": model,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--deck", default="demo", choices=["lands", "demo", "burn_vs_bears"])
    ap.add_argument("--timesteps", type=int, default=60_000)
    ap.add_argument("--eval-games", type=int, default=400)
    ap.add_argument("--n-envs", type=int, default=8)
    ap.add_argument("--save", default=None)
    args = ap.parse_args()

    res = train_and_eval(args.deck, args.timesteps, args.eval_games, args.n_envs)
    t, b = res["trained"], res["baseline"]
    print(f"deck={args.deck}  timesteps={args.timesteps}  eval_games={args.eval_games}")
    print(f"  random baseline  win-rate {b['win_rate']:.3f}  (W{b['win']}/D{b['draw']}/L{b['loss']})")
    print(f"  trained policy   win-rate {t['win_rate']:.3f}  (W{t['win']}/D{t['draw']}/L{t['loss']})")
    print(f"  beats random:    {'YES' if t['win_rate'] > b['win_rate'] + 0.05 else 'no'}")
    if args.save:
        res["model"].save(args.save)
        print(f"  saved → {args.save}")


if __name__ == "__main__":
    main()
