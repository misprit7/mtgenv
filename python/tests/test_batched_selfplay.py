"""Batched inference + batched self-play vec env (GYM_PLAN §6, task #41).

These cover correctness, not speed (the speed comparison lives in ``throughput.py`` /
``bench_infer.py``): the batched primitive must agree with per-sample inference, and the lockstep
pump must produce legal, terminating self-play games and train under MaskablePPO.
"""

import glob
import os

import numpy as np
import pytest

torch = pytest.importorskip("torch")
from sb3_contrib import MaskablePPO  # noqa: E402

from mtgenv_gym import BatchedPolicy, BatchedSelfPlayVecEnv, MtgEnv  # noqa: E402
from mtgenv_gym.attn_policy import RelationalPointerPolicy  # noqa: E402 — mean-pool retired; attn is default


def _model(venv):
    return MaskablePPO(RelationalPointerPolicy, venv, n_steps=64, batch_size=64, verbose=0)


def test_batched_policy_matches_per_sample_and_masks():
    pool = "/tmp/mtgenv_pool_test_bp"
    os.makedirs(pool, exist_ok=True)
    for f in glob.glob(pool + "/*.zip"):
        os.remove(f)
    venv = BatchedSelfPlayVecEnv("demo", pool, 2, seed=0)
    model = _model(venv)
    bp = BatchedPolicy(model)

    obs_list, mask_list = [], []
    for s in range(6):
        e = MtgEnv(deck="demo")
        o, i = e.reset(seed=s)
        obs_list.append(o)
        mask_list.append(i["action_mask"])

    acts = bp.act(obs_list, mask_list, deterministic=True)
    assert acts.shape == (6,)
    assert all(bool(mask_list[k][acts[k]]) for k in range(6)), "batched action must be legal"
    # batched == per-sample (correct stacking / no cross-row leakage)
    single = np.array(
        [model.predict(obs_list[k], action_masks=mask_list[k], deterministic=True)[0] for k in range(6)]
    ).reshape(-1)
    assert (acts == single).all()

    priors, values = bp.evaluate(obs_list, mask_list)
    assert priors.shape == (6, model.action_space.n) and values.shape == (6,)
    assert np.allclose(priors.sum(1), 1.0, atol=1e-4)
    assert max(priors[k][~mask_list[k]].sum() for k in range(6)) < 1e-6, "illegal actions get 0 prior"
    assert bp.act([], []).shape == (0,)
    venv.close()


def test_batched_selfplay_is_legal_and_terminates():
    pool = "/tmp/mtgenv_pool_test_bsp"
    os.makedirs(pool, exist_ok=True)
    for f in glob.glob(pool + "/*.zip"):
        os.remove(f)
    n = 8
    ve = BatchedSelfPlayVecEnv("demo", pool, n, p_random=1.0, seed=3)  # empty pool ⇒ random opponents
    obs = ve.reset()
    assert all(v.shape[0] == n for v in obs.values())

    rng = np.random.default_rng(0)
    rewards, done = [], 0
    for _ in range(4000):
        masks = np.stack(ve.env_method("action_masks"))
        acts = [int(rng.choice(np.flatnonzero(masks[i]))) for i in range(n)]
        ve.step_async(acts)
        _o, r, d, info = ve.step_wait()
        for i in range(n):
            if d[i]:
                done += 1
                rewards.append(float(r[i]))
                assert "terminal_observation" in info[i]
        if done >= 20:
            break
    assert done >= 20, "self-play games should terminate"
    assert set(rewards) <= {-1.0, 0.0, 1.0}, "sparse terminal reward only"
    ve.close()


def test_potential_shaping_is_bounded_pbrs():
    """Potential-based shaping (GYM_PLAN §5): Φ signs/bounds are sane, `coef=0` is an exact no-op,
    and with `coef>0` the per-step shaping is small + bounded while terminal stays ±1. No model
    needed (empty pool ⇒ random opponents)."""
    from mtgenv_gym.batched_selfplay import BatchedSelfPlayVecEnv as Bz

    G = 67

    def mk(my_life, opp_life, my_pow=0.0, opp_pow=0.0):
        g = np.zeros((1, G), np.float32)
        g[:, 16], g[:, 29] = my_life, opp_life
        bf = np.zeros((1, 32, 41), np.float32)
        bf[:, 0, 0], bf[:, 0, 1], bf[:, 0, 2] = 1, 1, my_pow   # my creature
        bf[:, 1, 0], bf[:, 1, 1], bf[:, 1, 2] = 1, 0, opp_pow  # opp creature
        return {"globals": g, "bf_feat": bf}

    phi = Bz._phi_batch
    assert abs(phi(mk(20, 20))[0]) < 1e-6           # symmetric → 0
    assert phi(mk(20, 5))[0] > 0 and phi(mk(5, 20))[0] < 0
    assert phi(mk(20, 20, my_pow=6))[0] > 0          # board lead → positive
    assert abs(phi(mk(9999, 0, my_pow=99))[0]) <= 1.0  # bounded

    pool = "/tmp/mtgenv_pool_test_shape"
    os.makedirs(pool, exist_ok=True)
    for f in glob.glob(pool + "/*.zip"):
        os.remove(f)

    def run(coef):
        ve = BatchedSelfPlayVecEnv("demo", pool, 8, p_random=1.0, seed=2, shaping_coef=coef)
        ve.reset()
        rng = np.random.default_rng(0)
        rs, ts = [], []
        for _ in range(300):
            m = ve._masks
            ve.step_async([int(rng.choice(np.flatnonzero(m[i]))) for i in range(8)])
            _o, r, d, _i = ve.step_wait()
            rs.append(r.copy())
            ts.append(d.copy())
        ve.close()
        return np.array(rs), np.array(ts)

    r0, t0 = run(0.0)
    r5, t5 = run(0.5)
    assert np.allclose(r0[~t0], 0.0), "coef=0 ⇒ no shaping (pure terminal)"
    assert np.abs(r5[~t5]).max() > 0, "coef>0 ⇒ intermediate shaping signal"
    assert np.abs(r5[~t5]).max() < 1.0, "shaping stays smaller than the ±1 terminal"
    assert set(np.round(r5[t5]).tolist()) <= {-1.0, 0.0, 1.0}, "terminal reward still ±1"


def test_maskable_ppo_trains_over_batched_vecenv():
    pool = "/tmp/mtgenv_pool_test_bsp_train"
    os.makedirs(pool, exist_ok=True)
    for f in glob.glob(pool + "/*.zip"):
        os.remove(f)
    ve = BatchedSelfPlayVecEnv("demo", pool, 8, p_random=0.2, seed=0)
    model = _model(ve)
    model.save(pool + "/ckpt_000000000")  # seed pool so the NN-opponent path is exercised
    model.learn(total_timesteps=1024, progress_bar=False)  # must not raise
    ve.close()
