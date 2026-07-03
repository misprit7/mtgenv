"""Stochastic MuZero (MLP) config for the swine matchup (M1 wiring / M2 smoke / M3 real run).

Run:  PYTHONPATH=../../python .venv/bin/python swine_stochastic_muzero_config.py [--smoke]

The env is a single-agent stochastic MDP (opponent + draws folded into env stochasticity), so
`env_type='not_board_games'` and `to_play=-1`. Chance is handled by the learned VQ ChanceEncoder
(`use_ture_chance_label_in_chance_encoder=False`) over a `chance_space_size` codebook — no
ground-truth chance label needed from the env. The legality mask rides in the obs dict and is
consumed by the MCTS (root + tree) via the policy's legal-action handling.

`--smoke` shrinks the net / sims / batch so training can be exercised on CPU or a brief GPU run
(M2). Defaults are the M3-real-run values (still modest; tune up if budget allows).
"""

from __future__ import annotations

import sys
from easydict import EasyDict

import lz_patches  # noqa: F401  — monkeypatches LightZero v0.2.0 stochastic-muzero bugs on import

SMOKE = "--smoke" in sys.argv
# Parallel collection across worker processes — the main throughput lever (collection is CPU/MCTS
# bound, GPU sits ~15%). 'base' is serial. Verify workers can import this module + mtgenv_gym.
SUBPROCESS = "--subprocess" in sys.argv
# --shaping = the M3 cold-start remedy (v1.1): dense potential-based reward (card-dominant Phi, the
# same one PPO uses) + a search-depth bump, to escape the "value=-0.8 everywhere" basin the pure
# sparse-reward run fell into. Policy-invariant; eval is still raw ±1. Separate TB run (3.1).
SHAPING = "--shaping" in sys.argv
# coef 0.1 = the gym's OWN proven PBRS coefficient (batched_selfplay), validated vs 583k real games
# and matching what PPO trains with (keeps the comparison fair). Constant (no anneal) at this budget.
REWARD_SHAPING = 0.1 if SHAPING else 0.0

# ── swine env-measured dims (see README M0) ──────────────────────────────────────────────────
OBS_DIM = 2650          # flattened Dict obs (1966 spec + 684 card-id one-hots)
ACTION_DIM = 98         # factored Discrete action space
CHANCE_SPACE = 8        # VQ chance-code codebook size (learned, unsupervised)

# ── budget knobs ─────────────────────────────────────────────────────────────────────────────
collector_env_num = 2 if SMOKE else 8
n_episode = 2 if SMOKE else 8
evaluator_env_num = 2 if SMOKE else 5
num_simulations = 8 if SMOKE else (100 if SHAPING else 50)   # deeper search helps escape cold-start
update_per_collect = 2 if SMOKE else 100
batch_size = 32 if SMOKE else 256
latent_state_dim = 64 if SMOKE else 256
max_env_step = int(2e3) if SMOKE else int(250e3)   # M3 target (with a 3.5h wall-clock hard cap)
reanalyze_ratio = 0.0

# M3 real run keeps ckpt/data/log under the (gitignored) local tb/ dir; a symlink surfaces the TB
# events under /tmp/mtgenv_tb/3.0-muzero-swine (the user's single versioned TensorBoard). New algo
# family -> 3.0 major bump per house convention.
if SMOKE:
    exp_name = "tb/mtg_swine_stochastic_muzero_smoke"
elif SHAPING:
    exp_name = "tb/3.1-muzero-swine-shaped"
else:
    exp_name = "tb/3.0-muzero-swine"

swine_stochastic_muzero_config = dict(
    exp_name=exp_name,
    env=dict(
        env_id='mtg_swine',
        deck='swine',
        opponent='random',       # M3 self-play swaps in a frozen-self opponent
        max_decisions=3000,
        agent_seat=0,
        reward_shaping=REWARD_SHAPING,   # 0.0 pure sparse (default); 0.3 with --shaping (v1.1)
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
        # THE key stochastic-MuZero switch: learn chance codes with the VQ ChanceEncoder rather
        # than reading a ground-truth chance label (our draws/opponent are not enumerable). See README.
        use_ture_chance_label_in_chance_encoder=False,
        model_path=None,
        cuda=True,
        env_type='not_board_games',
        # CRITICAL for our masked, factored action space: the set of legal actions varies per node
        # (2..98). 'fixed_action_space' (the default, for Atari) stores raw variable-length MCTS visit
        # distributions -> inhomogeneous policy-target array -> crash. 'varied_action_space' scatters
        # each distribution into a fixed length-98 vector via the legal-action indices. (Same setting
        # LightZero's board-game configs use.)
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
        learning_rate=0.003,
        ssl_loss_weight=2,
        grad_clip_value=0.5,
        replay_buffer_size=int(1e5) if SMOKE else int(1e6),
        eval_freq=int(100) if SMOKE else int(2e3),
    ),
)
swine_stochastic_muzero_config = EasyDict(swine_stochastic_muzero_config)
main_config = swine_stochastic_muzero_config

swine_stochastic_muzero_create_config = dict(
    env=dict(
        type='mtg_swine',
        import_names=['swine_lightzero_env'],  # module on sys.path (run from this dir)
    ),
    env_manager=dict(type='subprocess' if SUBPROCESS else 'base'),  # --subprocess = parallel collection
    policy=dict(
        type='stochastic_muzero',
        import_names=['lzero.policy.stochastic_muzero'],
    ),
)
swine_stochastic_muzero_create_config = EasyDict(swine_stochastic_muzero_create_config)
create_config = swine_stochastic_muzero_create_config


if __name__ == "__main__":
    from lzero.entry import train_muzero
    train_muzero([main_config, create_config], seed=0, max_env_step=max_env_step)
