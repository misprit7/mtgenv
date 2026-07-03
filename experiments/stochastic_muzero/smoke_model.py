"""M1 wiring: the StochasticMuZeroModelMLP builds at swine dims and does forward inferences on
real env observations. No training — this just proves the net is dimensionally wired to the env.

    PYTHONPATH=../../python .venv/bin/python smoke_model.py
"""
from __future__ import annotations

import numpy as np
import torch

from swine_lightzero_env import MtgSwineEnv
from swine_stochastic_muzero_config import main_config
from lzero.model.stochastic_muzero_model_mlp import StochasticMuZeroModelMLP


def main():
    mcfg = dict(main_config.policy.model)
    print("model cfg:", {k: mcfg[k] for k in
          ('observation_shape', 'action_space_size', 'chance_space_size', 'model_type', 'latent_state_dim')})
    model = StochasticMuZeroModelMLP(**mcfg).eval()
    n_params = sum(p.numel() for p in model.parameters())
    print(f"model built: {n_params/1e6:.2f}M params")

    # Real observations from the env (a small batch).
    env = MtgSwineEnv(main_config.env)
    env.seed(0, dynamic_seed=False)
    obs = env.reset()
    batch = np.stack([obs['observation'] for _ in range(4)]).astype(np.float32)
    x = torch.from_numpy(batch)
    assert x.shape == (4, mcfg['observation_shape']), x.shape

    with torch.no_grad():
        init = model.initial_inference(x)
        # NOTE: at the root there is no reward, so MuZero returns reward as a zero list, not a tensor.
        print("initial_inference: value", tuple(init.value.shape),
              "reward", np.shape(init.reward),
              "policy_logits", tuple(init.policy_logits.shape),
              "latent", tuple(init.latent_state.shape))
        assert init.policy_logits.shape == (4, mcfg['action_space_size'])

        # afterstate/chance path: encode chance from next obs, then recurrent inference.
        # option for the afterstate->state transition is a chance one-hot (chance_space_size);
        # for the state->afterstate transition it is an action one-hot (action_space_size).
        latent = init.latent_state
        action_oh = torch.nn.functional.one_hot(
            torch.zeros(4, dtype=torch.long), mcfg['action_space_size']).float()
        # state --action--> afterstate  (afterstate=True path)
        aft = model.recurrent_inference(latent, action_oh, afterstate=True)
        print("recurrent(afterstate): reward", tuple(aft.reward.shape),
              "policy_logits", tuple(aft.policy_logits.shape),
              "next", tuple(aft.latent_state.shape))
        # afterstate --chance--> next state (afterstate=False path, option=chance one-hot)
        chance_oh = torch.nn.functional.one_hot(
            torch.zeros(4, dtype=torch.long), mcfg['chance_space_size']).float()
        nxt = model.recurrent_inference(aft.latent_state, chance_oh, afterstate=False)
        print("recurrent(chance):     reward", tuple(nxt.reward.shape),
              "policy_logits", tuple(nxt.policy_logits.shape),
              "next", tuple(nxt.latent_state.shape))

        # VQ chance encoder infers the chance code from a PAIR of consecutive obs (obs_t ++ obs_t+1),
        # so its input is 2*observation_shape -> one-hot code of size chance_space_size.
        pair = torch.cat([x, x], dim=1)
        enc, onehot = model.chance_encode(pair)
        print("chance_encode(pair 2*obs): encoding", tuple(enc.shape), "onehot", tuple(onehot.shape))
        assert onehot.shape == (4, mcfg['chance_space_size'])
    print("MODEL WIRING OK")


if __name__ == "__main__":
    main()
