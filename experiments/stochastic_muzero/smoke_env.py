"""M1 smoke: the LightZero adapter env resets/steps and plays a full random episode.

    PYTHONPATH=../../python .venv/bin/python smoke_env.py
"""
from __future__ import annotations

import numpy as np

from swine_lightzero_env import MtgSwineEnv
from ding.utils import ENV_REGISTRY
from ding.envs import BaseEnvTimestep


def main():
    # (1) registry resolves it by the name a LightZero config would use.
    cls = ENV_REGISTRY.get('mtg_swine')
    assert cls is MtgSwineEnv, cls
    print("registry 'mtg_swine' ->", cls.__name__)

    cfg = MtgSwineEnv.default_config()
    cfg.max_decisions = 3000
    env = MtgSwineEnv(cfg)
    env.seed(123, dynamic_seed=False)
    print("env:", env)
    print("observation_space:", env.observation_space.shape,
          "action_space:", env.action_space.n)

    # (2) reset returns the LightZero obs dict.
    obs = env.reset()
    assert set(obs.keys()) == {'observation', 'action_mask', 'to_play', 'timestep'}, obs.keys()
    assert obs['observation'].shape == (env.observation_space.shape[0],)
    assert obs['action_mask'].shape == (env.action_space.n,)
    assert obs['to_play'] == -1
    print(f"reset ok: obs {obs['observation'].shape} dtype={obs['observation'].dtype}, "
          f"mask sum={int(obs['action_mask'].sum())}, legal={env.legal_actions[:8]}...")

    # (3) play a full random-legal episode; confirm BaseEnvTimestep + terminal reward.
    steps, done, ts = 0, False, None
    rng = np.random.default_rng(0)
    while not done:
        legal = np.nonzero(obs['action_mask'])[0]
        assert legal.size >= 1, "empty mask"
        a = int(rng.choice(legal))
        ts = env.step(a)
        assert isinstance(ts, BaseEnvTimestep)
        obs, done = ts.obs, ts.done
        steps += 1
        if steps > 20000:
            raise SystemExit("episode did not terminate")
    print(f"episode done in {steps} sub-decisions; reward={float(ts.reward):+.1f}; "
          f"eval_episode_return={ts.info.get('eval_episode_return')}")
    assert 'eval_episode_return' in ts.info
    assert float(ts.reward) in (-1.0, 0.0, 1.0)

    # (4) a few episodes to sanity-check length distribution + reward spread.
    lens, rews = [], []
    for i in range(8):
        obs = env.reset()
        d, n, tr = False, 0, 0.0
        while not d:
            legal = np.nonzero(obs['action_mask'])[0]
            ts = env.step(int(rng.choice(legal)))
            obs, d, n = ts.obs, ts.done, n + 1
            tr = ts.info.get('eval_episode_return', tr)
        lens.append(n); rews.append(tr)
    print(f"8 episodes: len mean={np.mean(lens):.0f} min={min(lens)} max={max(lens)}; "
          f"rewards={rews}")
    print("SMOKE OK")


if __name__ == "__main__":
    main()
