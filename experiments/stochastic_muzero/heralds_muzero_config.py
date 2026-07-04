"""PLAIN MuZero (NOT stochastic) on heralds — the localizing diagnostic.

The stochastic-muzero runs collapse on trivially-learnable heralds even with temp=1.0. The single
biggest unknown in our setup is the **VQ ChanceEncoder**: our "chance" (opponent replies + draws) is
NOT enumerable, yet stochastic-muzero compresses it into only chance_space_size=8 discrete codes and
unrolls a learned afterstate/chance dynamics through it. If that machinery is corrupting the dynamics
model, MCTS unrolls garbage -> collapse. Plain MuZero has NO chance machinery — it folds all env
stochasticity into a single (averaged) dynamics model, the standard way to run MuZero on a stochastic
single-agent POMDP. If PLAIN MuZero learns heralds where STOCHASTIC didn't, the chance machinery is
the culprit. Same env/obs/action/reward otherwise, so it's a clean fork.

Run:  PYTHONPATH=../../python .venv/bin/python heralds_muzero_config.py [--max-steps N] [--exp NAME] [--fix]
"""
from __future__ import annotations

import sys
from easydict import EasyDict


def _argval(flag, cast, default):
    if flag in sys.argv:
        return cast(sys.argv[sys.argv.index(flag) + 1])
    return default


SMOKE = "--smoke" in sys.argv
FIX = "--fix" in sys.argv   # manual_temperature_decay (temp 1.0 early)
NOSSL = "--nossl" in sys.argv   # disable self-supervised consistency loss (test latent-collapse)

OBS_DIM = 2593
ACTION_DIM = 98

collector_env_num = 2 if SMOKE else 8
n_episode = 2 if SMOKE else 8
evaluator_env_num = 2 if SMOKE else 5
num_simulations = _argval("--sims", int, 8 if SMOKE else 50)
batch_size = 32 if SMOKE else 256
latent_state_dim = 64 if SMOKE else 256
max_env_step = _argval("--max-steps", int, int(2e3) if SMOKE else int(200e3))
exp_name = _argval("--exp", str, "tb/3.2-muzero-heralds-plain")

main_config = EasyDict(dict(
    exp_name=exp_name,
    env=dict(
        env_id='mtg_swine', deck='heralds', opponent='random', max_decisions=3000, agent_seat=0,
        # --shaping F -> card-dominant PBRS potential (dense value signal to counter the sparse-reward
        # value collapse that drives the low-index/PASS attractor). Policy-invariant; eval stays raw ±1.
        reward_shaping=_argval("--shaping", float, 0.0), shaping_gamma=0.997,
        collector_env_num=collector_env_num, evaluator_env_num=evaluator_env_num,
        n_evaluator_episode=evaluator_env_num, manager=dict(shared_memory=False),
    ),
    policy=dict(
        model=dict(
            observation_shape=OBS_DIM, action_space_size=ACTION_DIM, model_type='mlp',
            latent_state_dim=latent_state_dim, self_supervised_learning_loss=(not NOSSL),
            discrete_action_encoding_type='one_hot', norm_type='BN', res_connection_in_dynamics=True,
        ),
        model_path=None, cuda=True, env_type='not_board_games', action_type='varied_action_space',
        manual_temperature_decay=FIX,
        game_segment_length=200, num_simulations=num_simulations, reanalyze_ratio=0.0,
        num_unroll_steps=5, td_steps=5, discount_factor=0.997,
        n_episode=n_episode,
        update_per_collect=_argval("--up", int, 2 if SMOKE else 100), batch_size=batch_size,
        optim_type='Adam', learning_rate=_argval("--lr", float, 0.003),
        ssl_loss_weight=(0 if NOSSL else 2), grad_clip_value=0.5,
        replay_buffer_size=int(1e5) if SMOKE else int(1e6),
        eval_freq=int(100) if SMOKE else int(2e3),
    ),
))
create_config = EasyDict(dict(
    env=dict(type='mtg_swine', import_names=['swine_lightzero_env']),
    env_manager=dict(type='base'),
    policy=dict(type='muzero', import_names=['lzero.policy.muzero']),
))


if __name__ == "__main__":
    from lzero.entry import train_muzero
    train_muzero([main_config, create_config], seed=0, max_env_step=max_env_step)
