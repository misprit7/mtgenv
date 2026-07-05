"""AUDIT — what does training DO to the net? Compare the raw network policy-prior + value at fixed
game states across the collapse (iteration_0 untrained vs a post-collapse checkpoint).

No MCTS, no training — just `model.initial_inference(obs)` → (value, policy_logits). Reports value
and softmax(policy_logits) over the LEGAL actions at (a) the Mulligan node [96=mull,97=keep] and
(b) the first multi-legal Priority node [0=PASS,...]. If training drives the prior onto 96/PASS and
the value strongly negative, we SEE the collapse the search then amplifies.

Run: PYTHONPATH=../../python .venv/bin/python audit_policy_shift.py --config heralds_plain \
       --ckpts <run>/ckpt/iteration_0.pth.tar <run>/ckpt/iteration_1280.pth.tar --latent 128
"""
from __future__ import annotations
import argparse, os
import numpy as np, torch

import lz_patches  # noqa
from muzero_metrics import build_policy, _obs_keys
from mtgenv_gym import MtgEnv


def _flat(o, keys):
    return np.concatenate([np.asarray(o[k], dtype=np.float32).ravel() for k in keys]).astype(np.float32)


def walk_to(env, seed, request, min_legal, rng):
    obs, info = env.reset(seed=seed)
    for _ in range(60):
        legal = np.flatnonzero(np.asarray(info["action_mask"], dtype=bool))
        if str(info.get("request")) == request and legal.size >= min_legal:
            return obs, info
        obs, r, term, trunc, info = env.step(int(rng.choice(legal)))
        if term or trunc:
            return None, None
    return None, None


def probe_ckpt(config, ck, deck, keys, states, device, latent):
    policy, _ = build_policy(config, ck, device, latent=latent)
    model = policy._learn_model if hasattr(policy, "_learn_model") else policy._model
    model.eval()
    from lzero.policy import inverse_scalar_transform
    out = {}
    with torch.no_grad():
        for name, (obs, legal) in states.items():
            data = torch.from_numpy(_flat(obs, keys)[None, :]).to(device)
            no = model.initial_inference(data)
            v = inverse_scalar_transform(no.value, policy._cfg.model.support_scale).item() \
                if hasattr(policy._cfg.model, "support_scale") else float(no.value.mean())
            logits = no.policy_logits[0].cpu().numpy()
            leg = np.asarray(legal)
            pr = np.exp(logits[leg] - logits[leg].max()); pr = pr / pr.sum()
            out[name] = (v, leg, pr)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--config", default="heralds_plain")
    ap.add_argument("--ckpts", nargs="+", required=True)
    ap.add_argument("--latent", type=int, default=128)
    ap.add_argument("--device", default="cuda" if torch.cuda.is_available() else "cpu")
    args = ap.parse_args()

    from muzero_metrics import _load_config
    main_config, _ = _load_config(args.config)
    deck = main_config.env.deck
    keys = _obs_keys(deck)
    env = MtgEnv(deck=deck, opponent="random")
    rng = np.random.default_rng(0)

    # fixed states
    om, im = walk_to(env, 42, "Mulligan", 2, rng)
    op, ip = walk_to(env, 7, "Priority", 4, rng)
    states = {}
    if om is not None:
        states["Mulligan[96=mull,97=keep]"] = (om, np.flatnonzero(np.asarray(im["action_mask"], bool)))
    if op is not None:
        states["Priority[0=PASS,...]"] = (op, np.flatnonzero(np.asarray(ip["action_mask"], bool)))

    print(f"config={args.config} deck={deck} latent={args.latent}")
    for ck in args.ckpts:
        res = probe_ckpt(args.config, ck, deck, keys, states, args.device, args.latent)
        print(f"\n== {os.path.basename(os.path.dirname(os.path.dirname(ck)))}/{os.path.basename(ck)} ==")
        for name, (v, leg, pr) in res.items():
            top = sorted(zip(leg.tolist(), pr.tolist()), key=lambda x: -x[1])[:5]
            print(f"  {name:26s} value={v:+.3f}  prior(top)={[ (int(a),round(p,3)) for a,p in top ]}")


if __name__ == "__main__":
    main()
