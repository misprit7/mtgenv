"""Shared LightZero config builder for muzero2 — used by both train.py (training) and mz_policy.py
(evalkit adapter). Pure function, no argv side effects, so importing it never parses the command line.

The eval-time config MUST match the trained checkpoint's model (latent, heads, obs/action dims,
action_type) or load_state_dict / MCTS shapes mismatch. Keeping ONE builder guarantees that.

Algorithms (``algo`` param):
  * ``muzero`` / ``gumbel``            — the 3.5-era A/B pair (plain MuZero, Gumbel-MuZero).
  * ``efficientzero``                  — EfficientZero (SSL consistency + value-prefix LSTM).  train_muzero.
  * ``stochastic_muzero``              — Stochastic MuZero (chance-node world model).           train_muzero.
  * ``unizero``                        — UniZero (transformer world model).                     train_unizero.

The first four share the MuZero-family policy/config shape (``_build_muzero_family``) and route through
``lzero.entry.train_muzero``. UniZero has a different config shape (a ``world_model_cfg`` transformer
block, AdamW lr, no MLP ``latent_state_dim``) and routes through ``lzero.entry.train_unizero`` — see
``_build_unizero`` and the deviations recorded there.
"""
from __future__ import annotations

from easydict import EasyDict

# algo -> LightZero policy ``type`` (the create_config policy type / import path).
POLICY_TYPE = {
    'gumbel': 'gumbel_muzero',
    'muzero': 'muzero',
    'efficientzero': 'efficientzero',
    'stochastic_muzero': 'stochastic_muzero',
    'unizero': 'unizero',
}
# algos whose training entry is train_unizero (vs train_muzero for the rest).
UNIZERO_ALGOS = {'unizero'}


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
    # ── EfficientZero / Stochastic MuZero extras ─────────────────────────────────────────────────
    lstm_hidden_size: int = 256, chance_space_size: int = 32,
    # ── UniZero (transformer world model) extras — see _build_unizero ────────────────────────────
    embed_dim: int = 256, num_layers: int = 4, num_heads: int = 4, infer_context_length: int = 4,
):
    assert algo in POLICY_TYPE, algo
    obs_dim, action_dim = probe_dims(deck)

    env_cfg = dict(
        env_id='mtg_lz', deck=deck, opponent='random', max_decisions=3000, agent_seat=0,
        reward_shaping=reward_shaping, shaping_gamma=0.997,
        collector_env_num=collector_env_num, evaluator_env_num=evaluator_env_num,
        n_evaluator_episode=evaluator_env_num, manager=dict(shared_memory=False),
    )
    create_config = EasyDict(dict(
        env=dict(type='mtg_lz', import_names=['mtg_lz_env']),
        env_manager=dict(type='base'),
        policy=dict(type=POLICY_TYPE[algo], import_names=[f'lzero.policy.{POLICY_TYPE[algo]}']),
    ))

    if algo in UNIZERO_ALGOS:
        policy = _build_unizero(
            obs_dim=obs_dim, action_dim=action_dim, num_simulations=num_simulations,
            reanalyze_ratio=reanalyze_ratio, update_per_collect=update_per_collect, td_steps=td_steps,
            num_unroll_steps=num_unroll_steps, learning_rate=learning_rate,
            game_segment_length=game_segment_length, n_episode=n_episode, batch_size=batch_size,
            replay_buffer_size=replay_buffer_size, eval_freq=eval_freq,
            save_ckpt_after_iter=save_ckpt_after_iter, cuda=cuda,
            collector_env_num=collector_env_num, evaluator_env_num=evaluator_env_num,
            embed_dim=embed_dim, num_layers=num_layers, num_heads=num_heads,
            infer_context_length=infer_context_length,
        )
    else:
        policy = _build_muzero_family(
            algo=algo, obs_dim=obs_dim, action_dim=action_dim, latent_state_dim=latent_state_dim,
            head_hidden=head_hidden, num_simulations=num_simulations, reanalyze_ratio=reanalyze_ratio,
            random_collect_episode_num=random_collect_episode_num, update_per_collect=update_per_collect,
            td_steps=td_steps, num_unroll_steps=num_unroll_steps, learning_rate=learning_rate,
            game_segment_length=game_segment_length, n_episode=n_episode, batch_size=batch_size,
            replay_buffer_size=replay_buffer_size, eval_freq=eval_freq,
            save_ckpt_after_iter=save_ckpt_after_iter, cuda=cuda,
            max_num_considered_actions=max_num_considered_actions,
            lstm_hidden_size=lstm_hidden_size, chance_space_size=chance_space_size,
        )

    main_config = EasyDict(dict(exp_name=exp_name, env=env_cfg, policy=policy))
    return main_config, create_config


def _build_muzero_family(
    *, algo, obs_dim, action_dim, latent_state_dim, head_hidden, num_simulations, reanalyze_ratio,
    random_collect_episode_num, update_per_collect, td_steps, num_unroll_steps, learning_rate,
    game_segment_length, n_episode, batch_size, replay_buffer_size, eval_freq, save_ckpt_after_iter,
    cuda, max_num_considered_actions, lstm_hidden_size, chance_space_size,
):
    """Config for the MuZero-family policies (muzero / gumbel / efficientzero / stochastic_muzero).
    All share the MLP representation + MuZero-style buffer and route through ``train_muzero``."""
    model = dict(
        observation_shape=obs_dim, action_space_size=action_dim, model_type='mlp',
        latent_state_dim=latent_state_dim, self_supervised_learning_loss=True,
        discrete_action_encoding_type='one_hot', norm_type='BN', res_connection_in_dynamics=True,
        reward_head_hidden_channels=list(head_hidden),
        value_head_hidden_channels=list(head_hidden),
        policy_head_hidden_channels=list(head_hidden),
    )
    if algo == 'efficientzero':
        # EfficientZero predicts a value-*prefix* via an LSTM in the dynamics net.
        model['lstm_hidden_size'] = lstm_hidden_size
    if algo == 'stochastic_muzero':
        # Chance-node world model: the after-state predicts a categorical chance outcome. MTG draws
        # (and the folded-in random opponent) are the stochasticity being modeled.
        model['chance_space_size'] = chance_space_size

    policy = dict(
        model=model, model_path=None, cuda=cuda, env_type='not_board_games',
        action_type='varied_action_space', game_segment_length=game_segment_length,
        num_simulations=num_simulations, reanalyze_ratio=reanalyze_ratio,
        random_collect_episode_num=random_collect_episode_num,
        num_unroll_steps=num_unroll_steps, td_steps=td_steps, discount_factor=0.997,
        n_episode=n_episode, update_per_collect=update_per_collect, batch_size=batch_size,
        optim_type='Adam', learning_rate=learning_rate, ssl_loss_weight=2, grad_clip_value=0.5,
        replay_buffer_size=replay_buffer_size, eval_freq=eval_freq,
        learn=dict(learner=dict(hook=dict(save_ckpt_after_iter=save_ckpt_after_iter))),
    )
    if algo == 'gumbel':
        policy['max_num_considered_actions'] = max_num_considered_actions
    if algo == 'stochastic_muzero':
        # infer the chance label from consecutive latents rather than a ground-truth env signal
        # (the env exposes no chance label). Matches the prior arc's heralds stochastic config.
        policy['use_ture_chance_label_in_chance_encoder'] = False
    return policy


def _build_unizero(
    *, obs_dim, action_dim, num_simulations, reanalyze_ratio, update_per_collect, td_steps,
    num_unroll_steps, learning_rate, game_segment_length, n_episode, batch_size, replay_buffer_size,
    eval_freq, save_ckpt_after_iter, cuda, collector_env_num, evaluator_env_num,
    embed_dim, num_layers, num_heads, infer_context_length,
):
    """UniZero config (transformer latent world model). Routes through ``train_unizero``.

    Deviations from the 3.5 MuZero recipe (recorded here + in the run notes):
      * optim AdamW lr 1e-4 (UniZero default) — the 3.5 Adam/3e-3 destabilizes the transformer WM.
      * ``latent_state_dim`` does not apply; capacity is the transformer (embed_dim/num_layers/num_heads,
        vector defaults from LightZero's lunarlander_disc_unizero config: 256 / 4 / 4).
      * ``random_collect_episode_num`` is dropped — LightZeroRandomPolicy has no unizero pipeline; UniZero
        relies on its own policy-entropy exploration instead of a random-collect floor.
      * ``reanalyze_ratio`` defaults to 0 (UniZero's transformer reanalyze path differs); the caller may
        still pass one.
      * ``num_unroll_steps`` sets the world-model block/token budget (2 tokens per step: obs + action).
    """
    lr = learning_rate if learning_rate != 0.003 else 0.0001  # never carry the MuZero 3e-3 into the WM
    model = dict(
        observation_shape=obs_dim, action_space_size=action_dim, model_type='mlp',
        self_supervised_learning_loss=True, discrete_action_encoding_type='one_hot', norm_type='BN',
        world_model_cfg=dict(
            continuous_action_space=False,
            max_blocks=num_unroll_steps, max_tokens=2 * num_unroll_steps,
            context_length=2 * infer_context_length,
            device='cuda' if cuda else 'cpu',
            action_space_size=action_dim, group_size=8,
            num_layers=num_layers, num_heads=num_heads, embed_dim=embed_dim,
            env_num=max(collector_env_num, evaluator_env_num),
            collector_env_num=collector_env_num, evaluator_env_num=evaluator_env_num,
            obs_type='vector', norm_type='BN', rotary_emb=False,
        ),
    )
    policy = dict(
        model=model, model_path=None, cuda=cuda, env_type='not_board_games',
        action_type='varied_action_space', game_segment_length=game_segment_length,
        num_simulations=num_simulations, reanalyze_ratio=reanalyze_ratio,
        num_unroll_steps=num_unroll_steps, td_steps=td_steps, discount_factor=0.997,
        n_episode=n_episode, update_per_collect=update_per_collect, batch_size=batch_size,
        optim_type='AdamW', learning_rate=lr, grad_clip_value=5,
        piecewise_decay_lr_scheduler=False, target_update_freq=100,
        replay_buffer_size=replay_buffer_size, eval_freq=eval_freq,
        collector_env_num=collector_env_num, evaluator_env_num=evaluator_env_num,
        learn=dict(learner=dict(hook=dict(save_ckpt_after_iter=save_ckpt_after_iter))),
    )
    return policy
