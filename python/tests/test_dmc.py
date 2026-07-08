"""Deep Monte-Carlo arm (python/mtgenv_gym/dmc.py) — unit coverage of the pure parts (return
labelling, ε schedule, replay buffer, slot layout) + torch coverage of the action-as-input net
(forward shape, illegal-action masking via the policy adapter) + a tiny end-to-end smoke (collect a
handful of transitions and take one DMC gradient step on heralds).
"""

import numpy as np
import pytest

from mtgenv_gym.dmc import (
    EpsilonSchedule,
    ReplayBuffer,
    compute_returns,
    greedy_from_q,
    slot_layout,
)


# ── pure: Monte-Carlo return labelling ──────────────────────────────────────────────────────────
def test_compute_returns_zero_sum():
    # seat-0 win (+1): seat-0 transitions +1, seat-1 transitions -1.
    assert compute_returns([0, 1, 0, 1], 1.0) == [1.0, -1.0, 1.0, -1.0]
    # seat-0 loss (-1): mirror.
    assert compute_returns([0, 1, 0], -1.0) == [-1.0, 1.0, -1.0]
    # draw (0): everyone 0.
    assert compute_returns([0, 1, 0, 1], 0.0) == [0.0, 0.0, 0.0, 0.0]
    # a transition's label is the WHOLE game's outcome (undiscounted): same |value| everywhere.
    rets = compute_returns([0] * 5, 1.0)
    assert rets == [1.0] * 5


# ── pure: ε schedule ────────────────────────────────────────────────────────────────────────────
def test_epsilon_schedule_linear_then_flat():
    s = EpsilonSchedule(start=0.9, end=0.1, decay_steps=1000)
    assert s.value(0) == pytest.approx(0.9)
    assert s.value(500) == pytest.approx(0.5)          # halfway
    assert s.value(1000) == pytest.approx(0.1)         # end
    assert s.value(5000) == pytest.approx(0.1)         # clamped flat after decay
    assert s.value(-10) == pytest.approx(0.9)          # clamped before 0


# ── pure: slot layout tiles [0, action_dim) exactly like codec.rs ────────────────────────────────
def test_slot_layout_matches_codec():
    lay = slot_layout(max_hand=16, max_perm=32, max_stack=8, action_dim=98)
    assert lay["hand"] == (1, 16)          # HAND_BASE=1
    assert lay["perm"] == (17, 32)         # PERM_BASE=17
    assert lay["stack"] == (51, 8)         # STACK_BASE=51
    assert lay["action_dim"] == 98
    with pytest.raises(AssertionError):    # a wrong total is caught loudly (obs↔codec desync)
        slot_layout(16, 32, 8, 99)


# ── pure: greedy over legal with random tie-break ────────────────────────────────────────────────
def test_greedy_respects_mask_and_argmax():
    q = np.array([5.0, 1.0, 9.0, 2.0])
    mask = np.array([True, True, False, True])   # slot 2 (the true max) is ILLEGAL
    assert greedy_from_q(q, mask, np.random.default_rng(0)) == 0  # best legal is slot 0
    # ties among legal maxima resolve to one of the tied indices (never an illegal one).
    q2 = np.array([3.0, 3.0, 3.0, 0.0])
    mask2 = np.array([True, True, False, True])
    picks = {greedy_from_q(q2, mask2, np.random.default_rng(i)) for i in range(20)}
    assert picks.issubset({0, 1})            # slot 2 illegal, slot 3 lower → only the tied {0,1}
    # the tie-break must ALSO accept the np.random *module* (the eval path evalkit seeds globally),
    # not just a Generator — regression for the .integers-vs-module crash.
    assert greedy_from_q(q2, mask2, np.random) in {0, 1}


# ── pure: replay buffer ring ─────────────────────────────────────────────────────────────────────
def test_replay_buffer_ring_and_sample():
    sample_obs = {"globals": np.zeros(4, np.float32), "bf_ids": np.zeros(3, np.int64)}
    buf = ReplayBuffer(capacity=3, sample_obs=sample_obs)
    for i in range(5):  # overflow the ring of 3
        buf.add({"globals": np.full(4, i, np.float32), "bf_ids": np.full(3, i, np.int64)},
                action=i, target=float(-i))
    assert buf.size == 3
    obs, act, tgt = buf.sample(64, np.random.default_rng(0))
    # only the last three writes (i in {2,3,4}) survive the ring.
    assert set(act.tolist()) == {2, 3, 4}
    assert set(tgt.tolist()) == {-2.0, -3.0, -4.0}
    # per-key obs stays consistent with its action (globals row == action value).
    assert np.all(obs["globals"][:, 0] == act.astype(np.float32))


# ── torch: net forward + masking + a real gradient step on heralds ───────────────────────────────
@pytest.mark.slow
def test_net_forward_and_masking():
    import torch

    from mtgenv_gym.dmc import DMCNet, DMCPolicy
    from mtgenv_gym.env import MtgEnv

    env = MtgEnv(deck="heralds", opponent="external")
    net = DMCNet(env.observation_space, env.action_dim)
    env.ext_reset(0)
    obs, mask = env.ext_obs(), env.ext_mask()

    pol = DMCPolicy(net, device="cpu")
    # greedy + sampled both return a LEGAL action for a real decision.
    for mode in ("greedy", "sample"):
        a = pol.act([obs], [mask], mode=mode)[0]
        assert mask[a], f"{mode} chose an illegal action {a}"
    # forward shape is (B, action_dim).
    from mtgenv_gym.dmc import obs_to_tensors, stack_obs
    q = net(obs_to_tensors(stack_obs([obs, obs]), "cpu"))
    assert q.shape == (2, env.action_dim)


@pytest.mark.slow
def test_collect_and_one_update():
    """End-to-end: mirror-self-play collect a few transitions, then one MSE step drops the loss."""
    import torch
    import torch.nn.functional as F

    from mtgenv_gym.dmc import (
        DMCNet,
        ReplayBuffer,
        SelfPlayCollector,
        obs_to_tensors,
    )
    from mtgenv_gym.env import MtgEnv

    probe = MtgEnv(deck="heralds", opponent="external")
    probe.ext_reset(0)
    net = DMCNet(probe.observation_space, probe.action_dim)
    buf = ReplayBuffer(8192, probe.ext_obs())
    coll = SelfPlayCollector("heralds", num_envs=8, seed=1)
    # Transitions only reach the buffer when a game FINISHES (episode-complete MC labelling), so
    # collect in rounds until some games have completed and flushed.
    total = 0
    for _ in range(200):
        total += coll.collect(net, buf, "cpu", min_transitions=512, epsilon=0.5)
        if buf.size >= 256:
            break
    assert buf.size >= 256 and coll.games_done > 0
    assert coll.env_steps == total

    opt = torch.optim.Adam(net.parameters(), lr=1e-2)
    obs, act, tgt = buf.sample(256, np.random.default_rng(0))
    obs_t = obs_to_tensors(obs, "cpu")
    act_t = torch.as_tensor(act).long().view(-1, 1)
    tgt_t = torch.as_tensor(tgt).float()

    def loss_now():
        q = net(obs_t)
        return F.mse_loss(q.gather(1, act_t).squeeze(1), tgt_t)

    before = loss_now().item()
    for _ in range(30):
        loss = loss_now()
        opt.zero_grad()
        loss.backward()
        opt.step()
    assert loss_now().item() < before  # the net can fit the MC targets it collected
