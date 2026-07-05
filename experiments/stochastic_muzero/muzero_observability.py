"""OBSERVABILITY BUNDLE for a MuZero run — PPO-parity TB tags + one replay per checkpoint.

For each checkpoint of a run it:
  * plays N games vs random-legal with **fair-greedy** MCTS (argmax visit counts, ties broken by the
    network policy PRIOR instead of by lowest action index — the low-index tie-break otherwise makes a
    weak net read 0 by always picking PASS/mulligan; see DEBUG AUDIT), computing win-rate +
    `productive_rate` + `attack_rate` (from decision_stats, mirroring tracked_stats.py);
  * plays N games with a **sampled** policy (visit^(1/0.25), NO exploration noise) for the sampled
    win-rate — the honest learning-signal curve (PPO's 0.97 is greedy, so greedy is the fair headline);
  * records ONE greedy self-mirror game to data/replays via MtgEnv.export_replay with the standard
    aitrain naming, so MuZero games show up in the web lobby next to PPO's;
  * writes scalars into the run's OWN TB dir at the checkpoint's ENV-STEP under the EXACT PPO tags
    (`selfplay/winrate_vs_random`, `stats/productive_rate`, `stats/attack_rate`) so TB overlays them
    with the 2.x PPO runs, plus MuZero-specific `selfplay/winrate_vs_random_sampled`.

Env-step per checkpoint is recovered from the run's training log (iteration_N -> latest
total_envstep_count before that ckpt save). Run:
  PYTHONPATH=../../python .venv/bin/python muzero_observability.py --config swine_stoch \
      --ckpt-dir tb/3.0-muzero-swine/ckpt --run-log m3_run.log --logdir tb/3.0-muzero-swine/log/serial \
      --run-name 3.0-muzero-swine --games 100
"""
from __future__ import annotations

import argparse, glob, os, re, time
import numpy as np
import torch

import lz_patches  # noqa
from muzero_metrics import build_policy, _obs_keys, _load_config
from mtgenv_gym import MtgEnv

# data/replays (mirror mtgenv_gym.replays.REPLAY_DIR without importing it — that module needs sb3,
# which isn't in this isolated venv). experiments/stochastic_muzero/ -> repo root is ../../.
REPLAY_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "data", "replays"))

_BF_PRESENT, _BF_MINE, _BF_POWER = 0, 1, 2


def ckpt_envstep_map(run_log):
    """Parse a LightZero run log → {iteration:int -> env_step:int} using the latest total_envstep_count
    seen before each 'iteration_N.pth.tar' save (and iteration_0 -> 0)."""
    m = {0: 0}
    if not run_log or not os.path.exists(run_log):
        return m
    last_step = 0
    for line in open(run_log, errors="ignore"):
        s = re.search(r"total_envstep_count:\s*(\d+)", line)
        if s:
            last_step = int(s.group(1)); continue
        c = re.search(r"iteration_(\d+)\.pth\.tar", line)
        if c:
            m[int(c.group(1))] = last_step
    return m


class MZAgent:
    """MuZero MCTS agent exposing fair-greedy and sampled action selection over an MtgEnv obs dict."""

    def __init__(self, policy, keys, device, temp=0.25):
        self._eval = policy.eval_mode
        self._model = getattr(policy, "_eval_model", None) or getattr(policy, "_learn_model")
        self._support = policy._cfg.model.support_scale if hasattr(policy._cfg.model, "support_scale") else None
        self._keys = keys; self._device = device; self._temp = temp

    def _flat(self, o):
        return np.concatenate([np.asarray(o[k], dtype=np.float32).ravel() for k in self._keys]).astype(np.float32)

    def _search(self, obs, mask):
        data = torch.from_numpy(self._flat(obs)[None, :]).to(self._device)
        out = self._eval.forward(data, [np.asarray(mask, dtype=np.int64)], [-1])
        dist = np.asarray(out[0]['visit_count_distributions'], dtype=np.float64)  # over legal, in legal order
        legal = np.nonzero(np.asarray(mask, dtype=np.int64))[0]
        return data, dist, legal

    def act_greedy(self, obs, mask):
        """argmax visits, ties broken by the network prior over the same legal set (fair-greedy)."""
        data, dist, legal = self._search(obs, mask)
        with torch.no_grad():
            no = self._model.initial_inference(data)
            logits = no.policy_logits[0].detach().cpu().numpy()[legal]
        prior = np.exp(logits - logits.max()); prior = prior / prior.sum()
        # visits dominate when peaked; prior only decides (near-)ties.
        return int(legal[int(np.argmax(dist + 1e-6 * prior))])

    def act_sampled(self, obs, mask):
        """sample from visits^(1/temp) — the sampled-eval learning-signal policy (no exploration noise)."""
        _, dist, legal = self._search(obs, mask)
        p = np.power(dist, 1.0 / self._temp)
        if p.sum() <= 0:
            p = np.ones_like(p)
        p = p / p.sum()
        return int(legal[int(np.random.choice(len(legal), p=p))])

    # MtgEnv opponent protocol
    def act(self, obs, mask):
        return self.act_greedy(obs, mask)


def _swine_attacking(bf):
    atk = bf.shape[1] - 5
    return bool(((bf[:, _BF_PRESENT] > 0.5) & (bf[:, _BF_MINE] < 0.5) & (bf[:, _BF_POWER] == 3) & (bf[:, atk] > 0.5)).any())


def eval_checkpoint(agent, deck, games, sampled_games, seed0=5_000_000):
    """Fair-greedy win-rate + productive_rate + attack_rate over `games`; sampled win-rate over `sampled_games`."""
    env = MtgEnv(deck=deck, opponent="random")
    g_wins = 0; pnum = pden = anum = aden = 0.0
    for i in range(games):
        obs, info = env.reset(seed=seed0 + i); done = False; r = 0.0
        while not done:
            a = agent.act_greedy(obs, info["action_mask"])
            obs, r, term, trunc, info = env.step(a); done = term or trunc
            st = info.get("decision_stats")
            if st:
                pnum += float(st.get("productive_taken", 0)); pden += float(st.get("productive_legal", 0))
                anum += float(st.get("attack_declared", 0)); aden += float(st.get("attack_eligible", 0))
        g_wins += 1 if r > 0.5 else 0
    s_wins = 0
    for i in range(sampled_games):
        obs, info = env.reset(seed=seed0 + 900_000 + i); done = False; r = 0.0
        while not done:
            a = agent.act_sampled(obs, info["action_mask"])
            obs, r, term, trunc, info = env.step(a); done = term or trunc
        s_wins += 1 if r > 0.5 else 0
    return dict(
        winrate_vs_random=g_wins / games,
        winrate_vs_random_sampled=(s_wins / sampled_games) if sampled_games else float("nan"),
        productive_rate=(pnum / pden) if pden > 0 else float("nan"),
        attack_rate=(anum / aden) if aden > 0 else float("nan"),
    )


def record_replay(agent, deck, step, run_name, seed=777):
    """One greedy game — MuZero (seat 0) vs a RANDOM-legal opponent — → data/replays via export_replay
    (v2 COMPACT delta form, same writer PPO uses; drives the MCTS agent, not model.predict).
    Opponent is RANDOM (not a self-mirror): an untrained/collapsed policy PASSes, and a self-mirror of
    two passers does nothing for ~deck-out-many turns → a >70MB replay. A random opponent actively
    develops a board and ends the game in ~40-60 decisions, keeping every replay bounded (PPO range)
    and matching the win-rate-vs-random protocol. max_decisions=600 is a belt-and-suspenders cap."""
    env = MtgEnv(deck=deck, record_replay=True, replay_step=int(step), opponent="random", max_decisions=600)
    obs, info = env.reset(seed=seed + int(step)); done = False
    while not done:
        a = agent.act_greedy(obs, info["action_mask"])
        obs, _r, term, trunc, info = env.step(a); done = term or trunc
    tag = f"MuZero@{run_name}:{step}"
    try:
        return env.export_replay(REPLAY_DIR, int(time.time() * 1000), names=[tag, "random"], decks=[deck, deck], run_name=run_name)
    except Exception as e:
        print(f"  [replay] skipped ({type(e).__name__}: {e})"); return None


def _iter_of(path):
    m = re.search(r"iteration_(\d+)", os.path.basename(path))
    return int(m.group(1)) if m else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--config", required=True, choices=["swine_stoch", "heralds_stoch", "heralds_plain", "swine_plain"])
    ap.add_argument("--deck", default=None)
    ap.add_argument("--ckpt-dir", required=True)
    ap.add_argument("--run-log", default=None, help="training log to recover env-step per checkpoint")
    ap.add_argument("--logdir", required=True, help="run's TB dir (…/log/serial) to append scalars to")
    ap.add_argument("--run-name", required=True)
    ap.add_argument("--games", type=int, default=100)
    ap.add_argument("--sampled-games", type=int, default=60)
    ap.add_argument("--latent", type=int, default=256)
    ap.add_argument("--sims", type=int, default=None)
    ap.add_argument("--no-replay", action="store_true")
    ap.add_argument("--device", default="cuda" if torch.cuda.is_available() else "cpu")
    args = ap.parse_args()

    main_config, _ = _load_config(args.config)
    deck = args.deck or main_config.env.deck
    keys = _obs_keys(deck)
    step_map = ckpt_envstep_map(args.run_log)

    from torch.utils.tensorboard import SummaryWriter
    writer = SummaryWriter(args.logdir)

    ckpts = sorted(glob.glob(os.path.join(args.ckpt_dir, "iteration_*.pth.tar")), key=lambda p: _iter_of(p) or 0)
    max_step = max(step_map.values()) if step_map else 0
    # ckpt_best -> the run's final env-step (its own headline point at the end of the curve).
    best = os.path.join(args.ckpt_dir, "ckpt_best.pth.tar")
    eval_list = [(ck, _iter_of(ck), step_map.get(_iter_of(ck), _iter_of(ck))) for ck in ckpts]
    if os.path.exists(best):
        eval_list.append((best, "best", max_step))
    print(f"run={args.run_name} config={args.config} deck={deck} ckpts={len(eval_list)} games={args.games}(+{args.sampled_games} sampled)")
    tags_written = set()
    replayed_steps = set()  # one replay per distinct env-step (ckpt_best often shares the final step)
    for ck, it, step in eval_list:
        policy, _ = build_policy(args.config, ck, args.device, latent=args.latent, sims=args.sims)
        agent = MZAgent(policy, keys, args.device)
        m = eval_checkpoint(agent, deck, args.games, args.sampled_games)
        rp = None
        if not args.no_replay and step not in replayed_steps:
            rp = record_replay(agent, deck, step, args.run_name)
            replayed_steps.add(step)
        for k, v in m.items():
            tag = f"selfplay/{k}" if k.startswith("winrate") else f"stats/{k}"
            writer.add_scalar(tag, v, step); tags_written.add(tag)
        print(f"  iter={it} step={step}: greedy={m['winrate_vs_random']:.3f} sampled={m['winrate_vs_random_sampled']:.3f} "
              f"prod={m['productive_rate']:.3f} atk={m['attack_rate']:.3f}  replay={os.path.basename(rp) if rp else '—'}")
    writer.flush(); writer.close()
    print("TAGS WRITTEN:", sorted(tags_written))


if __name__ == "__main__":
    main()
