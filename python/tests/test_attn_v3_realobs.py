"""Real-obs gates for the v3 attn extractor — require the v3 engine (skipped on a v2 build).

Complements the synthetic test_attn_v3.py with the checks the lead required against the REAL swine
obs (the 4.7 lesson: synthetic tests missed the pointer scale bug). Collects real v3 observations from
a random-legal rollout and asserts: shapes/NaNs, every edge endpoint is a present row, the mask is a
subset of the obs candidate flags, the packed gather matches the full path, and — the HARD gate — the
per-slot-family logit magnitudes at LEGAL positions are balanced (no family scale-suppressed).
"""

import numpy as np
import pytest

pytest.importorskip("torch")
mtg_py = pytest.importorskip("mtg_py", reason="mtg_py extension not built")
import torch  # noqa: E402

_IS_V3 = "edges" in {n for (n, *_rest) in mtg_py.PyGame.obs_spec()}
pytestmark = pytest.mark.skipif(not _IS_V3, reason="v3 engine not installed (no `edges` in obs_spec)")

from mtgenv_gym import MtgEnv  # noqa: E402
from mtgenv_gym.attn_policy import RelationalPointerPolicy  # noqa: E402
from mtgenv_gym.codec_layout import slot_layout  # noqa: E402

_BF_CAND = 42  # is_decision_candidate column (v3 bf_feat cols 0-44 == v2's first 45)


def _collect(steps=600, seed=0):
    env = MtgEnv(deck="swine")
    rng = np.random.default_rng(seed)
    obs_list, masks = [], []
    o, _ = env.reset(seed=seed)
    for step in range(steps):
        m = env.action_masks()
        obs_list.append({k: np.asarray(v).copy() for k, v in o.items()})
        masks.append(m.copy())
        o, _r, term, trunc, _i = env.step(int(rng.choice(np.flatnonzero(m))))
        if term or trunc:
            o, _ = env.reset(seed=step + 1000)
    tb = {k: torch.as_tensor(np.stack([b[k] for b in obs_list])) for k in obs_list[0]}
    return env, tb, torch.as_tensor(np.stack(masks))


def test_v3_real_obs_shapes_edges_and_mask_agreement():
    env, tb, mask = _collect()
    B = mask.shape[0]
    for k, v in tb.items():
        assert torch.isfinite(v.float()).all(), f"{k} has NaN/inf"
    mp, mh, ms = 256, 16, 8
    N = mp + mh + ms + 3
    present = torch.zeros(B, N, dtype=torch.bool)
    present[:, :mp] = tb["bf_feat"][..., 0] > 0.5
    present[:, mp:mp + mh] = tb["hand_feat"][..., 0] > 0.5
    present[:, mp + mh:mp + mh + ms] = tb["stack_feat"][..., 0] > 0.5
    present[:, mp + mh + ms:] = True
    e = tb["edges"]
    valid = e[..., 0] >= 0
    assert int(valid.sum()) > 0, "rollout produced no edges (combat never happened)"
    src, dst = e[..., 0].clamp(0, N - 1), e[..., 1].clamp(0, N - 1)
    bidx = torch.arange(B).unsqueeze(1).expand_as(src)
    assert present[bidx[valid], src[valid]].all(), "an edge src_row is not a present row"
    assert present[bidx[valid], dst[valid]].all(), "an edge dst_row is not a present row"
    # mask ⊆ candidate on combat decisions (legal PERM slot ⇒ flagged is_decision_candidate)
    lay = slot_layout(mh, mp, ms, mask.shape[1])
    pbase, pcnt = lay["perm"]
    combat = torch.isin(tb["globals"][:, 43:64].argmax(1), torch.tensor([9, 10]))
    seen = 0
    for i in torch.nonzero(combat).squeeze(1).tolist():
        legal = set(torch.nonzero(mask[i, pbase:pbase + pcnt]).squeeze(1).tolist())
        cand = set(torch.nonzero(tb["bf_feat"][i, :, _BF_CAND] > 0.5).squeeze(1).tolist())
        assert legal <= cand, f"obs {i}: a legal PERM slot is not a candidate"
        seen += 1
    assert seen > 0, "no combat decisions collected"


def test_v3_real_obs_packed_matches_full_and_scale_balance():
    env, tb, mask = _collect()
    pol = RelationalPointerPolicy(env.observation_space, env.action_space, lambda _: 3e-4)
    pol.eval()
    d = pol.features_extractor.d_model

    def run(flag):
        pol.features_extractor.enc.gather_present = flag
        with torch.no_grad():
            f = pol.extract_features(tb)
            return f, pol.value_net(f), pol.action_net(f)

    (f0, v0, l0), (f1, v1, l1) = run(False), run(True)
    # packed-v3 == full-v3 on everything behavioral
    assert torch.allclose(f0[:, :d], f1[:, :d], atol=1e-5), "state diverged"
    assert torch.allclose(v0, v1, atol=1e-5), "value diverged"
    assert torch.allclose(l0[mask], l1[mask], atol=1e-5), "a legal logit diverged"
    big = torch.finfo(l0.dtype).min
    assert (torch.where(mask, l0, big).argmax(1) == torch.where(mask, l1, big).argmax(1)).all()

    # HARD scale-balance: per-family mean|logit| at LEGAL positions within ~5x (no family suppressed).
    lay = slot_layout(16, 256, 8, mask.shape[1])
    fams = {"COMMIT": [lay["commit"][0]],
            "PERM": range(lay["perm"][0], lay["perm"][0] + lay["perm"][1]),
            "HAND": range(lay["hand"][0], lay["hand"][0] + lay["hand"][1]),
            "PLAYER": range(lay["player"][0], lay["player"][0] + lay["player"][1]),
            "YESNO": [lay["yes"][0], lay["no"][0]]}
    mags = {}
    for name, idxs in fams.items():
        idxs = list(idxs)
        sel = mask[:, idxs]
        if int(sel.sum()) >= 5:
            mags[name] = float(l0[:, idxs][sel].abs().mean())
    assert len(mags) >= 3, f"too few testable families: {list(mags)}"
    ratio = max(mags.values()) / max(min(mags.values()), 1e-6)
    assert ratio < 8.0, f"logit-family scale imbalance {ratio:.1f}x (4.7 bug risk): {mags}"
