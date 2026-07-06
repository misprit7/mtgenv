"""muzero2 — LightZero MuZero / Gumbel-MuZero on the mtgenv gym, done right (Track A).

The prior workstream (experiments/stochastic_muzero/, READ-ONLY) root-caused the collapse: a FLAT value
net (td_steps=5 too short for 30-60-sub-decision episodes) + a legal PASS at action index 0 (the
argmax/visit-count "passive" attractor) + sparse terminal reward + over-training the tiny early buffer,
with NO exploration floor. Its combined recipe (shaping 0.5 + td 40 + up 20) made PLAIN MuZero learn
trivial heralds (0 -> ~0.9) at only 60k steps but stalled on swine. This retry keeps that recipe and
adds every lever the prior run never tried (see mtg_config.build_configs docstring / README):

  * ALGO: Gumbel-MuZero (--algo gumbel) — policy improvement guaranteed even at low sims; root action
    selection is Gumbel-top-k + sequential halving (NOT argmax visit counts), dissolving the low-index/
    PASS passive-collapse attractor. (--algo muzero = plain MuZero A/B control.)
  * REANALYZE 0.5, EXPLORATION FLOOR (random_collect 32), CONSTANT PBRS shaping 0.5 (eval raw ±1),
    td 50 / unroll 10, latent 512, budget >=500k, game_segment_length 2000.

Run:
  PYTHONPATH=../../python .venv/bin/python train.py --algo gumbel --deck heralds \
      --exp /tmp/mtgenv_tb/3.4-gumbel-heralds --max-steps 500000
"""
from __future__ import annotations

import sys

import lz_patches  # noqa: F401  (enables the Gumbel random-collect exploration floor)
from mtg_config import build_configs


def _argval(flag, cast, default):
    if flag in sys.argv:
        return cast(sys.argv[sys.argv.index(flag) + 1])
    return default


SMOKE = "--smoke" in sys.argv
ALGO = _argval("--algo", str, "gumbel")
DECK = _argval("--deck", str, "heralds")
max_env_step = _argval("--max-steps", int, int(2e3) if SMOKE else int(500e3))
exp_name = _argval("--exp", str, f"/tmp/mtgenv_tb/dev-{ALGO}-{DECK}{'-smoke' if SMOKE else ''}")

# Smoke = tiny/fast wiring check (CPU-ok); real = the full recipe.
kw = dict(algo=ALGO, deck=DECK, exp_name=exp_name)
if SMOKE:
    kw.update(latent_state_dim=64, head_hidden=(32,), num_simulations=8, reanalyze_ratio=0.0,
              random_collect_episode_num=2, update_per_collect=2, td_steps=5, num_unroll_steps=5,
              collector_env_num=2, evaluator_env_num=2, n_episode=2, batch_size=32,
              replay_buffer_size=int(1e5), eval_freq=100, save_ckpt_after_iter=50,
              game_segment_length=800)   # > heralds game length so no split (avoids the :737 boundary bug)
else:
    kw.update(
        latent_state_dim=_argval("--latent", int, 512),
        num_simulations=_argval("--sims", int, 50),
        reanalyze_ratio=_argval("--reanalyze", float, 0.5),
        random_collect_episode_num=_argval("--random-collect", int, 32),
        update_per_collect=_argval("--up", int, 20),
        td_steps=_argval("--td", int, 50),
        num_unroll_steps=_argval("--unroll", int, 10),
        reward_shaping=_argval("--shaping", float, 0.5),
        learning_rate=_argval("--lr", float, 0.003),
        game_segment_length=_argval("--seg", int, 2000),
        save_ckpt_after_iter=_argval("--save-iter", int, 1000),
        max_num_considered_actions=_argval("--gumbel-actions", int, 16),
    )

main_config, create_config = build_configs(**kw)


if __name__ == "__main__":
    from lzero.entry import train_muzero
    p = main_config.policy
    print(f"[muzero2] algo={ALGO} deck={DECK} obs={p.model.observation_shape} act={p.model.action_space_size} "
          f"latent={p.model.latent_state_dim} reanalyze={p.reanalyze_ratio} rand_collect={p.random_collect_episode_num} "
          f"td={p.td_steps} unroll={p.num_unroll_steps} up={p.update_per_collect} "
          f"max_steps={max_env_step} exp={exp_name}", flush=True)
    train_muzero([main_config, create_config], seed=_argval("--seed", int, 0), max_env_step=max_env_step)
