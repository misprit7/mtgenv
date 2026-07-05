"""PLAIN MuZero on SWINE with the WINNING recipe (2026-07-04 debug) — TB 3.3-muzero-swine-combined.

The heralds falsifier proved the collapse is a MuZero-recipe mismatch (flat value net from td_steps=5
on long episodes + PASS/mulligan attractor + over-training), and that the combined recipe
`--shaping 0.5 --td 40 --up 20` makes MuZero LEARN (heralds → ~0.9). This applies that recipe to the
actual question: swine (3/3 trample vs the PPO chump-block failure). Plain MuZero — the audit showed
the stochastic VQ-chance machinery is not what helps here and collapses identically; plain is the
proven learner and folds draws into its dynamics model the standard way.

Run: PYTHONPATH=../../python .venv/bin/python swine_muzero_config.py [--shaping F --td N --up N --unroll N --latent N --max-steps N --exp NAME]
"""
from __future__ import annotations

import sys
from easydict import EasyDict


def _argval(flag, cast, default):
    if flag in sys.argv:
        return cast(sys.argv[sys.argv.index(flag) + 1])
    return default


SMOKE = "--smoke" in sys.argv

OBS_DIM = 2650          # swine flattened obs (audit M0): 1966 spec + 684 card-id one-hots
ACTION_DIM = 98

collector_env_num = 2 if SMOKE else 8
n_episode = 2 if SMOKE else 8
evaluator_env_num = 2 if SMOKE else 5
num_simulations = _argval("--sims", int, 8 if SMOKE else 50)
batch_size = 32 if SMOKE else 256
latent_state_dim = _argval("--latent", int, 64 if SMOKE else 256)
max_env_step = _argval("--max-steps", int, int(2e3) if SMOKE else int(150e3))
exp_name = _argval("--exp", str, "tb/3.3-muzero-swine-combined")

main_config = EasyDict(dict(
    exp_name=exp_name,
    env=dict(
        env_id='mtg_swine', deck='swine', opponent='random', max_decisions=3000, agent_seat=0,
        # winning recipe: card-dominant PBRS at coef 0.5 (3.1's 0.1 was too weak — anti-mulligan dense term)
        reward_shaping=_argval("--shaping", float, 0.5), shaping_gamma=0.997,
        collector_env_num=collector_env_num, evaluator_env_num=evaluator_env_num,
        n_evaluator_episode=evaluator_env_num, manager=dict(shared_memory=False),
    ),
    policy=dict(
        model=dict(
            observation_shape=OBS_DIM, action_space_size=ACTION_DIM, model_type='mlp',
            latent_state_dim=latent_state_dim, self_supervised_learning_loss=True,
            discrete_action_encoding_type='one_hot', norm_type='BN', res_connection_in_dynamics=True,
        ),
        model_path=None, cuda=True, env_type='not_board_games', action_type='varied_action_space',
        game_segment_length=200, num_simulations=num_simulations, reanalyze_ratio=0.0,
        # td_steps=40 (not 5): carry the terminal ±1 back across long factored episodes so the value net
        # becomes discriminative instead of flat-negative (the collapse root cause).
        num_unroll_steps=_argval("--unroll", int, 5), td_steps=_argval("--td", int, 40),
        discount_factor=0.997,
        n_episode=n_episode,
        update_per_collect=_argval("--up", int, 2 if SMOKE else 20),  # 20 (not 100): don't over-train the tiny early buffer
        batch_size=batch_size,
        optim_type='Adam', learning_rate=_argval("--lr", float, 0.003), ssl_loss_weight=2, grad_clip_value=0.5,
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
