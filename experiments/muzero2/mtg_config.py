"""Shared LightZero config builder for muzero2 — used by both train.py (training) and mz_policy.py
(evalkit adapter). Pure function, no argv side effects, so importing it never parses the command line.

The eval-time config MUST match the trained checkpoint's model (latent, heads, obs/action dims,
action_type) or load_state_dict / MCTS shapes mismatch. Keeping ONE builder guarantees that.
"""
from __future__ import annotations

from easydict import EasyDict


def probe_dims(deck: str):
    """obs_dim differs per deck (card-id one-hot width): heralds=2593, swine=2650. Probe, don't hardcode."""
    import numpy as np
    from mtgenv_gym import MtgEnv
    e = MtgEnv(deck=deck)
    o, _ = e.reset(seed=0)
    return int(sum(np.asarray(o[k]).size for k in o)), int(e.action_dim)


def build_configs(
    *, algo: str, deck: str, exp_name: str,
    latent_state_dim: int = 512, head_hidden=(64,), num_simulations: int = 50,
    reanalyze_ratio: float = 0.5, random_collect_episode_num: int = 32,
    update_per_collect: int = 20, td_steps: int = 50, num_unroll_steps: int = 10,
    reward_shaping: float = 0.5, learning_rate: float = 0.003, game_segment_length: int = 2000,
    collector_env_num: int = 8, evaluator_env_num: int = 3, n_episode: int = 8, batch_size: int = 256,
    replay_buffer_size: int = int(1e6), eval_freq: int = 4000, save_ckpt_after_iter: int = 1000,
    max_num_considered_actions: int = 16, cuda: bool = True,
):
    assert algo in ("gumbel", "muzero"), algo
    obs_dim, action_dim = probe_dims(deck)
    policy_type = 'gumbel_muzero' if algo == 'gumbel' else 'muzero'

    policy = dict(
        model=dict(
            observation_shape=obs_dim, action_space_size=action_dim, model_type='mlp',
            latent_state_dim=latent_state_dim, self_supervised_learning_loss=True,
            discrete_action_encoding_type='one_hot', norm_type='BN', res_connection_in_dynamics=True,
            reward_head_hidden_channels=list(head_hidden),
            value_head_hidden_channels=list(head_hidden),
            policy_head_hidden_channels=list(head_hidden),
        ),
        model_path=None, cuda=cuda, env_type='not_board_games', action_type='varied_action_space',
        game_segment_length=game_segment_length,
        num_simulations=num_simulations, reanalyze_ratio=reanalyze_ratio,
        random_collect_episode_num=random_collect_episode_num,
        num_unroll_steps=num_unroll_steps, td_steps=td_steps, discount_factor=0.997,
        n_episode=n_episode,
        update_per_collect=update_per_collect, batch_size=batch_size,
        optim_type='Adam', learning_rate=learning_rate, ssl_loss_weight=2, grad_clip_value=0.5,
        replay_buffer_size=replay_buffer_size,
        eval_freq=eval_freq,
        learn=dict(learner=dict(hook=dict(save_ckpt_after_iter=save_ckpt_after_iter))),
    )
    if algo == 'gumbel':
        policy['max_num_considered_actions'] = max_num_considered_actions

    main_config = EasyDict(dict(
        exp_name=exp_name,
        env=dict(
            env_id='mtg_lz', deck=deck, opponent='random', max_decisions=3000, agent_seat=0,
            reward_shaping=reward_shaping, shaping_gamma=0.997,
            collector_env_num=collector_env_num, evaluator_env_num=evaluator_env_num,
            n_evaluator_episode=evaluator_env_num, manager=dict(shared_memory=False),
        ),
        policy=policy,
    ))
    create_config = EasyDict(dict(
        env=dict(type='mtg_lz', import_names=['mtg_lz_env']),
        env_manager=dict(type='base'),
        policy=dict(type=policy_type, import_names=[f'lzero.policy.{policy_type}']),
    ))
    return main_config, create_config
