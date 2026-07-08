"""Equivalence gate for the present-row-gather fast path in ``_RelationalEncoder`` (attn_policy.py).

The encoder can run its attention layers over ONLY the present rows (packed across the batch, block-
diagonal + the same bf-bf relation bias) instead of the full N=1+256+16+8 tokens where ~95% are
padding. This is mathematically identical to the full masked attention FOR THE PRESENT ROWS — and
present rows are the only ones the pooled state + the legal action logits depend on (padded rows are
never legal actions, so their logits are always masked out before sampling and the PPO loss).

The gate runs on REAL swine observations from a live self-play rollout (not synthetic tensors — the
4.7 pointer-scale bug passed synthetic unit tests and only a real-env smoke caught it) and asserts the
two paths agree on: pooled features, value, and every LEGAL action logit, plus an identical greedy
masked action. Edge cases: an empty battlefield (opening obs) and a maximally-occupied board.
"""

import numpy as np
import pytest

pytest.importorskip("torch")
pytest.importorskip("mtg_py", reason="mtg_py extension not built")
import torch  # noqa: E402

from mtgenv_gym import MtgEnv  # noqa: E402
from mtgenv_gym.attn_policy import (  # noqa: E402
    RelationalPointerPolicy,
    _abstract_slot_indices,
    _entity_slots,
)


def _collect_real_obs(n_envs=8, steps=12, seed=0):
    """Real swine obs + action masks from a random-legal self-play rollout (includes empty boards,
    combat states, and heterogeneous present-counts within a batch)."""
    import tempfile

    from mtgenv_gym.fleet_selfplay import FleetSelfPlayVecEnv

    ve = FleetSelfPlayVecEnv("swine", tempfile.mkdtemp(), num_envs=n_envs, num_workers=4, seed=seed)
    obs = ve.reset()
    rng = np.random.default_rng(seed)
    obs_batches, masks = [], []
    def snap(o):
        obs_batches.append({k: np.asarray(v).copy() for k, v in o.items()})
        masks.append(np.stack(ve.env_method("action_masks")).copy())
    snap(obs)
    for _ in range(steps):
        m = masks[-1]
        acts = np.array([int(rng.choice(np.flatnonzero(m[i]))) for i in range(n_envs)])
        ve.step_async(acts)
        obs, _r, _d, _i = ve.step_wait()
        snap(obs)
    ve.close()
    keys = obs_batches[0].keys()
    big = {k: torch.as_tensor(np.concatenate([b[k] for b in obs_batches], axis=0)) for k in keys}
    mask = torch.as_tensor(np.concatenate(masks, axis=0))
    return big, mask


def _both_paths(pol, obs):
    out = {}
    for flag in (False, True):
        pol.features_extractor.enc.gather_present = flag
        with torch.no_grad():
            feats = pol.extract_features(obs)
            out[flag] = (feats, pol.value_net(feats), pol.action_net(feats))
    return out[False], out[True]


def test_gather_matches_full_path_on_real_swine_obs():
    obs, mask = _collect_real_obs()
    B, A = mask.shape
    mp = obs["bf_feat"].shape[1]
    env = MtgEnv(deck="swine")
    pol = RelationalPointerPolicy(env.observation_space, env.action_space, lambda _: 3e-4)
    pol.eval()
    (f0, v0, l0), (f1, v1, l1) = _both_paths(pol, obs)
    d = pol.features_extractor.d_model

    # pooled state + value: exact (padded keys are masked in the pool, so identical either way).
    assert torch.allclose(f0[:, :d], f1[:, :d], atol=1e-5), "pooled state diverged"
    assert torch.allclose(v0, v1, atol=1e-5), "value diverged"

    # every LEGAL action logit is identical → the masked policy (sampling + PPO loss) is unchanged.
    assert torch.allclose(l0[mask], l1[mask], atol=1e-5), "a legal-action logit diverged"
    # and the greedy masked action agrees on every obs.
    big_neg = torch.finfo(l0.dtype).min
    g0 = torch.where(mask, l0, big_neg).argmax(1)
    g1 = torch.where(mask, l1, big_neg).argmax(1)
    assert (g0 == g1).all(), "greedy masked action diverged"

    # present-row entity logits are exact too (not just legal ones); padded rows are never legal.
    slots = _entity_slots(16, mp, 8, A)
    present_pos = torch.zeros(B, A, dtype=torch.bool)
    present_pos[:, _abstract_slot_indices(slots, A)] = True
    for name, (base, cnt) in slots.items():
        present_pos[:, base:base + cnt] = obs[f"{name}_feat"][..., 0] > 0.5
    assert torch.allclose(l0[present_pos], l1[present_pos], atol=1e-5)
    assert int((mask & ~present_pos).sum()) == 0, "a padded row was legal (padded logits would matter)"

    # the rollout actually exercised an empty battlefield (opening obs) — exact there too.
    empty = (obs["bf_feat"][..., 0] > 0.5).sum(1) == 0
    assert empty.any(), "expected some empty-battlefield obs in the rollout"
    assert torch.allclose(l0[empty][mask[empty]], l1[empty][mask[empty]], atol=1e-5)


def test_gather_exact_when_no_padding():
    """Maximum occupancy: every row in every zone present → zero padded rows → the FULL logit vector
    is exact (a high-occupancy-only bug would surface here, past real obs' ~23-present cap)."""
    obs, _ = _collect_real_obs(n_envs=4, steps=3)
    obs = {k: v[:2].clone() for k, v in obs.items()}
    mp = obs["bf_feat"].shape[1]
    for z in ("bf", "hand", "stack"):
        obs[f"{z}_feat"][:, :, 0] = 1.0                      # mark all rows present
    if obs["bf_feat"].shape[2] > 45:                         # v2 only: distinct instance ids at col 45
        obs["bf_feat"][:, :, 45] = torch.arange(1, mp + 1).float()  # (v3 has no id cols — edges instead)
    env = MtgEnv(deck="swine")
    pol = RelationalPointerPolicy(env.observation_space, env.action_space, lambda _: 3e-4)
    pol.eval()
    (_, _, l0), (_, _, l1) = _both_paths(pol, obs)
    assert torch.allclose(l0, l1, atol=1e-5), "full-occupancy full-vector must be exact"
