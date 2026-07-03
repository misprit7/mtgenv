"""M4 evaluation: greedy Stochastic-MuZero play on swine — win-rate + combat-judgment metrics.

Two evaluations (mirroring the PPO baseline + `python/swine_blocks.py`):

  (a) win-rate vs random-legal opponent (same protocol as PPO's ~0.90 / random-baseline 0.535).
  (b) chump-block analysis in a greedy SELF-MIRROR (MuZero vs a greedy MuZero copy, so the attacks
      the blocker faces are purposeful): at every DeclareBlockers decision record my_life + block
      SHAPE (blocked? gang/double?), gated on whether a 3/3 trampling Swine is attacking. Bucketed
      by life >= 15 ("no pressure") — the exact failure PPO shows (chump-blocks the trampler ~94-97%
      even at high life). If MuZero's lookahead fixes it, that's the headline.

    PYTHONPATH=../../python .venv/bin/python eval_muzero_swine.py --model <ckpt.pth.tar> \
        --deck swine --winrate-games 500 --block-games 300

Greedy (deterministic MCTS argmax) throughout. Runs single-env (batch 1) MCTS per decision.
"""
from __future__ import annotations

import argparse
import numpy as np
import torch

import lz_patches  # noqa: F401  (timestep patch)
from ding.config import compile_config
from ding.policy import create_policy

from swine_stochastic_muzero_config import main_config, create_config
from mtgenv_gym import MtgEnv

# ── swine obs indices (match python/swine_blocks.py) ─────────────────────────────────────────
_MY_LIFE = 16          # globals index of the acting seat's life
_BF_PRESENT, _BF_MINE, _BF_POWER = 0, 1, 2


def _swine_attacking(bf: np.ndarray) -> bool:
    """True if an ENEMY (is_mine=0) power-3 creature is attacking (a trampling Swine); bears are 2/2."""
    atk = bf.shape[1] - 5   # attacking column (obs.rs layout; see swine_blocks.py)
    m = ((bf[:, _BF_PRESENT] > 0.5) & (bf[:, _BF_MINE] < 0.5)
         & (bf[:, _BF_POWER] == 3) & (bf[:, atk] > 0.5))
    return bool(m.any())


class MuZeroGreedy:
    """Greedy Stochastic-MuZero player: given an MtgEnv obs dict + mask, run MCTS and return argmax.

    Exposes ``act(obs_dict, mask) -> int`` so it can be BOTH the learner and the MtgEnv opponent
    (a greedy self-mirror)."""

    def __init__(self, policy, obs_keys, device):
        self._eval = policy.eval_mode
        self._keys = obs_keys
        self._device = device

    def _flatten(self, obs_dict) -> np.ndarray:
        return np.concatenate(
            [np.asarray(obs_dict[k], dtype=np.float32).ravel() for k in self._keys]
        ).astype(np.float32)

    def act(self, obs_dict, mask) -> int:
        data = torch.from_numpy(self._flatten(obs_dict)[None, :]).to(self._device)
        mask = np.asarray(mask, dtype=np.int64)
        out = self._eval.forward(data, [mask], [-1])
        return int(out[0]['action'])


def build_policy(model_path: str, device: str, latent_state_dim=None, num_simulations=None):
    # The model architecture must match the checkpoint's. Allow overriding the two knobs that differ
    # between the --smoke config (latent 64) and the real run (latent 256), so any ckpt loads.
    if latent_state_dim is not None:
        main_config.policy.model.latent_state_dim = int(latent_state_dim)
    if num_simulations is not None:
        main_config.policy.num_simulations = int(num_simulations)
    cfg = compile_config(main_config, seed=0, env=None, auto=True,
                         create_cfg=create_config, save_cfg=False)
    cfg.policy.device = device
    policy = create_policy(cfg.policy, model=None, enable_field=['learn', 'collect', 'eval'])
    policy.learn_mode.load_state_dict(torch.load(model_path, map_location=device))
    return policy


def _obs_keys(deck: str):
    e = MtgEnv(deck=deck)
    o, _ = e.reset(seed=0)
    return list(o.keys())


def win_rate(agent: MuZeroGreedy, deck: str, games: int, seed0=1_000_000):
    """MuZero (agent_seat) vs random-legal opponent."""
    env = MtgEnv(deck=deck, opponent="random")
    wins = draws = 0
    for i in range(games):
        obs, info = env.reset(seed=seed0 + i)
        done, r = False, 0.0
        while not done:
            a = agent.act(obs, info["action_mask"])
            obs, r, term, trunc, info = env.step(int(a))
            done = term or trunc
        if r > 0.5:
            wins += 1
        elif abs(r) < 0.5:
            draws += 1
    return wins / games, draws / games


def block_analysis(agent: MuZeroGreedy, deck: str, games: int, life_hi=15, seed0=3_000_000):
    """Greedy self-mirror; record (life, blocked?, double?, attackers_blocked, swine?) per block decision."""
    env = MtgEnv(deck=deck, opponent=agent)   # opponent is a greedy MuZero copy (same policy)
    rows = []
    for i in range(games):
        obs, info = env.reset(seed=seed0 + i)
        done = False
        while not done:
            life = float(obs["globals"][_MY_LIFE])
            swine = _swine_attacking(obs["bf_feat"])
            a = agent.act(obs, info["action_mask"])
            obs, _r, term, trunc, info = env.step(int(a))
            done = term or trunc
            st = info.get("decision_stats")
            if st and st.get("block_eligible", 0) > 0:
                rows.append((life,
                             1.0 if st["block_declared"] > 0 else 0.0,
                             1.0 if st["block_double"] > 0 else 0.0,
                             float(st["attackers_blocked"]),
                             1.0 if swine else 0.0))
    return np.array(rows) if rows else np.empty((0, 5))


def _line(name, r):
    if len(r) == 0:
        print(f"  {name:36s}: (none)")
        return
    block_rate = r[:, 1].mean()
    gang_rate = r[r[:, 1] > 0, 2].mean() if (r[:, 1] > 0).any() else float("nan")
    print(f"  {name:36s}: n={len(r):4d}  block_rate={block_rate:.3f}  gang_rate|blocked={gang_rate:.3f}")


def report_blocks(rows, life_hi):
    print(f"\n=== {len(rows)} DeclareBlockers decisions (greedy self-mirror) ===")
    if len(rows) == 0:
        print("  (no block decisions recorded)")
        return
    life, swine = rows[:, 0], rows[:, 4] > 0.5
    hi = life >= life_hi
    _line(f"SWINE attacking & life >= {life_hi}", rows[swine & hi])   # the user's exact concern
    _line(f"SWINE attacking & life <  {life_hi}", rows[swine & ~hi])
    _line("NO swine (bears in view)", rows[~swine])
    _line("ALL", rows)
    print(f"  -> CHUMP signal = high block_rate + low gang_rate in 'SWINE & life>={life_hi}'.")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", required=True, help="stochastic-muzero ckpt .pth.tar")
    ap.add_argument("--deck", default="swine")
    ap.add_argument("--winrate-games", type=int, default=500)
    ap.add_argument("--block-games", type=int, default=300)
    ap.add_argument("--life-hi", type=int, default=15)
    ap.add_argument("--device", default="cuda" if torch.cuda.is_available() else "cpu")
    ap.add_argument("--latent-state-dim", type=int, default=None,
                    help="override to match the checkpoint (smoke=64, real=256)")
    ap.add_argument("--num-simulations", type=int, default=None,
                    help="override eval-time MCTS sims (default: config value)")
    args = ap.parse_args()

    print(f"loading {args.model} on {args.device}")
    policy = build_policy(args.model, args.device, args.latent_state_dim, args.num_simulations)
    agent = MuZeroGreedy(policy, _obs_keys(args.deck), args.device)

    if args.winrate_games > 0:
        wr, dr = win_rate(agent, args.deck, args.winrate_games)
        print(f"\n=== win-rate vs random-legal ({args.winrate_games} games) ===")
        print(f"  win_rate={wr:.3f}  draw_rate={dr:.3f}   (PPO ~0.90 | random-vs-random 0.535)")

    if args.block_games > 0:
        rows = block_analysis(agent, args.deck, args.block_games, args.life_hi)
        report_blocks(rows, args.life_hi)


if __name__ == "__main__":
    main()
