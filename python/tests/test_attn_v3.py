"""Scaffold coverage for the v3 (OBS2 §7) path of RelationalPointerPolicy on a SYNTHETIC v3 obs space.

v3 (detected by an `edges` obs key): *_ids→*_grpid, no *_cardid, bf_feat 256×45 (relation cols dropped),
plus `edges` (128×4) consumed as a per-(type,direction) attention bias and `choice_feat` (16×12) as
pointer content for the abstract action buckets; you/opp/decision become always-present attention rows.

These are synthetic-obs gates (shapes, content flow, param parity, v2 untouched). The REAL gates —
real-env smoke + obs↔action edge-index agreement — run once the Rust v3 obs_spec is buildable.
"""

import numpy as np
import pytest

pytest.importorskip("torch")
import torch  # noqa: E402
from gymnasium import spaces  # noqa: E402

from mtgenv_gym.attn_policy import RelationalPointerPolicy  # noqa: E402
from mtgenv_gym.codec_layout import slot_layout  # noqa: E402

_ADIM = 322


def _v3_space():
    def box(shape, hi=np.inf):
        return spaces.Box(-hi, hi, shape, np.float32)
    return spaces.Dict({
        "globals": box((69,)),
        "bf_feat": box((256, 45)), "bf_grpid": spaces.Box(0, 1 << 30, (256,), np.int64),
        "hand_feat": box((16, 18)), "hand_grpid": spaces.Box(0, 1 << 30, (16,), np.int64),
        "stack_feat": box((8, 18)), "stack_grpid": spaces.Box(0, 1 << 30, (8,), np.int64),
        "decision_grpid": spaces.Box(0, 1 << 30, (1,), np.int64),
        "edges": spaces.Box(-1, 1 << 20, (128, 4), np.int32),
        "choice_feat": box((16, 12)),
    })


def _v3_obs(B=4, n_edges=6, seed=0):
    rng = np.random.default_rng(seed)
    sp = _v3_space()
    obs = {}
    for k, s in sp.spaces.items():
        if s.dtype == np.int64:
            obs[k] = torch.randint(0, 50, (B, *s.shape))
        elif s.dtype == np.int32:
            e = torch.full((B, 128, 4), -1, dtype=torch.int32)
            e[:, :n_edges, 0] = torch.randint(0, 280, (B, n_edges))    # src_row
            e[:, :n_edges, 1] = torch.randint(0, 280, (B, n_edges))    # dst_row
            e[:, :n_edges, 2] = torch.randint(0, 6, (B, n_edges))      # type 0..5
            obs[k] = e
        else:
            obs[k] = torch.randn(B, *s.shape)
    obs["bf_feat"][:, :10, 0] = 1.0        # 10 present bf rows
    obs["hand_feat"][:, :3, 0] = 1.0
    obs["stack_feat"][:, :1, 0] = 1.0
    return sp, obs


def _policy(sp):
    pol = RelationalPointerPolicy(sp, spaces.Discrete(_ADIM), lambda _: 3e-4)
    pol.eval()
    return pol


def test_v3_detected_and_forward_shapes():
    sp, obs = _v3_obs()
    pol = _policy(sp)
    assert pol.features_extractor.enc.v3 and pol.features_extractor.pack.v3
    with torch.no_grad():
        feats = pol.extract_features(obs)
        val = pol.value_net(feats)
        logits = pol.action_net(feats)
    assert logits.shape == (4, _ADIM) and torch.isfinite(logits).all()
    assert val.shape == (4, 1) and torch.isfinite(val).all()
    # v3 uses content pointers throughout — the v2-only families are gone; the v3 modules are present.
    names = set(dict(pol.named_parameters()))
    assert not any("abstract_q" in n or "globals_proj" in n or "w_block" in n or "w_attach" in n for n in names)
    assert any("edge_bias" in n for n in names) and any("choice_proj" in n for n in names)
    assert any("you_proj" in n for n in names) and any("decision_proj" in n for n in names)


def test_v3_choice_content_drives_modal_logits():
    """MODE/COLOR/NUMBER/YES-NO logits are q·choice_row — perturbing choice_feat must move them and
    leave the entity (PERM/HAND) logits unchanged (choice_feat doesn't enter the entity attention)."""
    sp, obs = _v3_obs()
    pol = _policy(sp)
    lay = slot_layout(16, 256, 8, _ADIM)
    with torch.no_grad():
        l0 = pol.action_net(pol.extract_features(obs))
        obs2 = {k: v.clone() for k, v in obs.items()}
        obs2["choice_feat"] = obs2["choice_feat"] + 5.0            # perturb only the choice content
        l1 = pol.action_net(pol.extract_features(obs2))
    mbase, mcnt = lay["mode"]
    assert (l0[:, mbase:mbase + mcnt] - l1[:, mbase:mbase + mcnt]).abs().max() > 1e-4, "MODE logits ignore choice_feat"
    pbase, pcnt = lay["perm"]
    assert torch.allclose(l0[:, pbase:pbase + pcnt], l1[:, pbase:pbase + pcnt], atol=1e-5), \
        "choice_feat must not affect entity (PERM) logits"


def test_v3_edge_bias_changes_attention():
    """With a nonzero edge bias, adding edges must change the encoder output (the bias is wired into the
    attention). With the zero-init bias it's a no-op — so we set it first, then compare edges vs none."""
    sp, obs = _v3_obs(n_edges=8)
    pol = _policy(sp)
    pol.features_extractor.enc.edge_bias.data.fill_(3.0)          # activate the bias
    no_edges = {k: v.clone() for k, v in obs.items()}
    no_edges["edges"] = torch.full_like(obs["edges"], -1)        # all-pad → no edges
    with torch.no_grad():
        f_edges = pol.extract_features(obs)
        f_none = pol.extract_features(no_edges)
    assert (f_edges - f_none).abs().max() > 1e-4, "edges do not affect the attention output"


def test_v3_param_parity_with_v2():
    """v3 stays within ±20% of the 4.9 arch (138,227): it swaps globals_proj/w_block/w_attach/abstract_q
    + cardid inputs for you/opp/decision/choice projections + the edge bias."""
    sp, _ = _v3_obs()
    n_v3 = sum(p.numel() for p in _policy(sp).parameters())
    baseline = 138_227
    assert 0.8 * baseline <= n_v3 <= 1.2 * baseline, f"v3 {n_v3:,} not within ±20% of 4.9 {baseline:,}"
