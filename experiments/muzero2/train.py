"""muzero2 — LightZero model-based tree-search training on the mtgenv gym (Track A + the 4.x arms).

Track A root-caused the prior collapse and proved plain MuZero learns heralds (0 -> 0.93) with the
"3.5 recipe" (latent 256, sims 50, td 40, unroll 5, up 20, reanalyze 0.25, random_collect 32, constant
PBRS shaping 0.5, lr 3e-3, game_segment_length 2000). The 4.x arms compare *other* model-based
tree-search families on the SAME heralds env / recipe:

  * --algo efficientzero       (4.0) — SSL consistency + value-prefix LSTM.            train_muzero
  * --algo stochastic_muzero   (4.1) — chance-node world model (MTG draw stochasticity). train_muzero
  * --algo unizero             (4.2) — transformer latent world model.                  train_unizero
  * --algo muzero / gumbel            — the Track-A A/B pair (kept for reference).

Run (real arm, 3.5 recipe applied via flags):
  PYTHONPATH=../../python .venv/bin/python train.py --algo efficientzero --deck heralds \
      --exp tb/4.0-ez-heralds --latent 256 --td 40 --unroll 5 --reanalyze 0.25 \
      --max-steps 500000 --notes "..."
"""
from __future__ import annotations

import os
import sys

import lz_patches  # noqa: F401  (Gumbel + Stochastic-MuZero random-collect exploration floor)
from mtg_config import build_configs, UNIZERO_ALGOS


def _argval(flag, cast, default):
    if flag in sys.argv:
        return cast(sys.argv[sys.argv.index(flag) + 1])
    return default


SMOKE = "--smoke" in sys.argv
ALGO = _argval("--algo", str, "gumbel")
DECK = _argval("--deck", str, "heralds")
max_env_step = _argval("--max-steps", int, int(2e3) if SMOKE else int(500e3))
exp_name = _argval("--exp", str, f"/tmp/mtgenv_tb/dev-{ALGO}-{DECK}{'-smoke' if SMOKE else ''}")

# Every real run must say what it is testing (user directive): --notes lands in
# /tmp/mtgenv_tb/<run>/description.md, which scripts/tb2aim.py lifts into the Aim run description.
NOTES = _argval("--notes", str, None)
if not NOTES and not SMOKE:
    sys.exit("train.py: --notes 'what this run tests' is required for non-smoke runs "
             "(it becomes the Aim run description)")

# Smoke = tiny/fast wiring check (CPU-ok); real = the full recipe.
kw = dict(algo=ALGO, deck=DECK, exp_name=exp_name)
if SMOKE:
    kw.update(latent_state_dim=64, head_hidden=(32,), num_simulations=8, reanalyze_ratio=0.0,
              random_collect_episode_num=2, update_per_collect=2, td_steps=5, num_unroll_steps=5,
              collector_env_num=2, evaluator_env_num=2, n_episode=2, batch_size=32,
              replay_buffer_size=int(1e5), eval_freq=100, save_ckpt_after_iter=50,
              game_segment_length=800,   # > heralds game length so no split (avoids the :737 boundary bug)
              lstm_hidden_size=64, chance_space_size=4,     # EZ / Stochastic-MuZero smoke sizes
              embed_dim=64, num_layers=2, num_heads=2, infer_context_length=4)  # UniZero smoke sizes
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
        lstm_hidden_size=_argval("--lstm", int, 256),
        chance_space_size=_argval("--chance", int, 32),
        embed_dim=_argval("--embed-dim", int, 256),
        num_layers=_argval("--layers", int, 4),
        num_heads=_argval("--heads", int, 4),
        infer_context_length=_argval("--infer-ctx", int, 4),
    )

main_config, create_config = build_configs(**kw)


if __name__ == "__main__":
    p = main_config.policy
    m = p.model
    cap = (f"latent={m.latent_state_dim}" if 'latent_state_dim' in m
           else f"embed={m.world_model_cfg.embed_dim}x{m.world_model_cfg.num_layers}L")
    recipe = (f"algo={ALGO} deck={DECK} obs={m.observation_shape} act={m.action_space_size} {cap} "
              f"reanalyze={p.reanalyze_ratio} rand_collect={p.get('random_collect_episode_num', 0)} "
              f"td={p.td_steps} unroll={p.num_unroll_steps} up={p.update_per_collect} lr={p.learning_rate} "
              f"max_steps={max_env_step} exp={exp_name}")
    print(f"[muzero2] {recipe}", flush=True)
    if NOTES:
        run_dir = os.path.join("/tmp/mtgenv_tb", os.path.basename(exp_name.rstrip("/")))
        os.makedirs(run_dir, exist_ok=True)
        with open(os.path.join(run_dir, "description.md"), "w") as f:
            f.write(f"{NOTES}\n\nRecipe: {recipe}\n")

    if ALGO in UNIZERO_ALGOS:
        from lzero.entry import train_unizero as train_entry
    else:
        from lzero.entry import train_muzero as train_entry
    train_entry([main_config, create_config], seed=_argval("--seed", int, 0), max_env_step=max_env_step)
