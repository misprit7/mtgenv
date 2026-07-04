"""Stochastic MuZero (MLP) config for the HERALDS matchup — the FALSIFIER (step 2).

Heralds mirror is a provably-optimal, trivially-learnable strategy (play land, cast herald, attack
all); PPO reaches ~0.972 vs random, productive_rate 1.0, attack_rate 1.0. If Stochastic MuZero
CANNOT clear >0.9 here, the swine negative is a *bug*, not deck difficulty — and this config is how
we prove it. It's a near-copy of the swine config with deck='heralds' and the heralds obs dim.

Run:
  PYTHONPATH=../../python .venv/bin/python heralds_stochastic_muzero_config.py [--smoke] \
      [--sims N] [--max-steps N] [--lr F] [--exp NAME]

`--smoke` = tiny CPU/GPU wiring check. Otherwise the real run (GPU). `--sims/--max-steps/--lr/--exp`
override the search depth / budget / learning-rate / TB run-name so a short falsifier smoke and the
full ~1h run share one file.
"""
from __future__ import annotations

import sys
from easydict import EasyDict

import lz_patches  # noqa: F401  — monkeypatches LightZero v0.2.0 stochastic-muzero bugs on import


def _argval(flag, cast, default):
    if flag in sys.argv:
        return cast(sys.argv[sys.argv.index(flag) + 1])
    return default


SMOKE = "--smoke" in sys.argv
SUBPROCESS = "--subprocess" in sys.argv

# ── heralds env-measured dims (audit: obs_dim 2593, action_dim 98) ────────────────────────────
OBS_DIM = 2593          # flattened Dict obs (heralds vocab = 2 unique cards -> smaller card-id one-hot)
ACTION_DIM = 98
CHANCE_SPACE = 8

# ── budget knobs ──────────────────────────────────────────────────────────────────────────────
collector_env_num = 2 if SMOKE else 8
n_episode = 2 if SMOKE else 8
evaluator_env_num = 2 if SMOKE else 5
num_simulations = _argval("--sims", int, 8 if SMOKE else 50)
update_per_collect = 2 if SMOKE else 100
batch_size = 32 if SMOKE else 256
latent_state_dim = 64 if SMOKE else 256
max_env_step = _argval("--max-steps", int, int(2e3) if SMOKE else int(300e3))
learning_rate = _argval("--lr", float, 0.003)
reanalyze_ratio = 0.0

_default_exp = ("tb/mtg_heralds_stochastic_muzero_smoke" if SMOKE else "tb/3.2-muzero-heralds")
exp_name = _argval("--exp", str, _default_exp)

heralds_stochastic_muzero_config = dict(
    exp_name=exp_name,
    env=dict(
        env_id='mtg_swine',            # the registered env type wraps ANY deck; deck below selects heralds
        deck='heralds',
        opponent='random',
        max_decisions=3000,
        agent_seat=0,
        reward_shaping=0.0,            # pure sparse ±1 — the falsifier should not need a crutch
        shaping_gamma=0.997,
        collector_env_num=collector_env_num,
        evaluator_env_num=evaluator_env_num,
        n_evaluator_episode=evaluator_env_num,
        manager=dict(shared_memory=False),
    ),
    policy=dict(
        model=dict(
            observation_shape=OBS_DIM,
            action_space_size=ACTION_DIM,
            chance_space_size=CHANCE_SPACE,
            model_type='mlp',
            latent_state_dim=latent_state_dim,
            self_supervised_learning_loss=True,
            discrete_action_encoding_type='one_hot',
            norm_type='BN',
            res_connection_in_dynamics=True,
        ),
        use_ture_chance_label_in_chance_encoder=False,
        model_path=None,
        cuda=True,
        env_type='not_board_games',
        action_type='varied_action_space',
        game_segment_length=200,
        num_simulations=num_simulations,
        reanalyze_ratio=reanalyze_ratio,
        num_unroll_steps=5,
        td_steps=5,
        discount_factor=0.997,
        n_episode=n_episode,
        update_per_collect=update_per_collect,
        batch_size=batch_size,
        optim_type='Adam',
        learning_rate=learning_rate,
        ssl_loss_weight=2,
        grad_clip_value=0.5,
        replay_buffer_size=int(1e5) if SMOKE else int(1e6),
        eval_freq=int(100) if SMOKE else int(2e3),
    ),
)
heralds_stochastic_muzero_config = EasyDict(heralds_stochastic_muzero_config)
main_config = heralds_stochastic_muzero_config

heralds_stochastic_muzero_create_config = dict(
    env=dict(
        type='mtg_swine',
        import_names=['swine_lightzero_env'],
    ),
    env_manager=dict(type='subprocess' if SUBPROCESS else 'base'),
    policy=dict(
        type='stochastic_muzero',
        import_names=['lzero.policy.stochastic_muzero'],
    ),
)
heralds_stochastic_muzero_create_config = EasyDict(heralds_stochastic_muzero_create_config)
create_config = heralds_stochastic_muzero_create_config


if __name__ == "__main__":
    from lzero.entry import train_muzero
    train_muzero([main_config, create_config], seed=0, max_env_step=max_env_step)
