# evalkit — algorithm-agnostic evaluation, metrics & logging for the mtgenv gym

One eval + metrics + logging stack shared by every RL algorithm. PPO uses it today; MCTS/AlphaZero,
DMC, MuZero, … plug in by writing **one thin `Policy` adapter**. Everything downstream — playing games,
the win-rate ladder, the canonical TensorBoard tags, JSON artifacts, deck analyzers, replays — is
written against the `Policy` protocol and reused unchanged.

> **User directive this implements:** *"always log the winrate vs random, ladder winrate (winrate vs
> 10%, 25%, 50%, etc), stats like attack rate, productive rate, avg turns, etc. Then for implementing
> new algorithms you write a wrapper."*

---

## Add an algorithm in ~20 lines

The **only** thing an algorithm must provide is a batched `act`:

```python
import numpy as np
from mtgenv_gym.evalkit import Arena, RandomPolicy, evaluate_checkpoint, BasePolicy

class MyPolicy(BasePolicy):                 # BasePolicy gives you a no-op reset()
    def __init__(self, net):
        self.net = net

    def act(self, obs_batch, mask_batch, *, mode="greedy", **kw):
        # obs_batch: list[K] of {name: np.ndarray} exactly as MtgEnv emits
        # mask_batch: list[K] of bool masks (shape (action_dim,))
        # return: np.ndarray (K,) of int actions
        logits = self.net.batch_logits(obs_batch)        # your forward
        for k, m in enumerate(mask_batch):
            logits[k][~m] = -np.inf                       # mask illegal
        if mode == "greedy":
            return logits.argmax(1)
        probs = softmax(logits)
        return np.array([np.random.choice(len(p), p=p) for p in probs])

# evaluate it — greedy AND sampled, win-rate + CI + avg turns + attack/productive/block rates:
res = Arena("swine").evaluate(MyPolicy(net), RandomPolicy(), n_games=200, seed=5_000_000)
print(res["greedy"], res["sample"])

# or log the full canonical battery into a TB run dir (scalars + JSON + replay), from any loop:
evaluate_checkpoint(MyPolicy(net), step=50_000, run_dir="/tmp/mtgenv_tb/my-run", deck="swine")
```

That's it. A **search** policy (MCTS/AZ) is the same shape — `act` runs its per-decision budget over
the batch of roots, keeps per-game trees keyed by the `env_indices` kwarg, and uses
`mtgenv_gym.inference.BatchedPolicy.evaluate` (masked priors + value in one forward) for leaf eval. See
`policy.SearchPolicy` for the documented skeleton.

---

## The `Policy` protocol

```python
def act(self, obs_batch, mask_batch, *, mode="greedy", env_indices=None) -> np.ndarray
def reset(self, env_indices, *, rng=None, game_seeds=None) -> None      # optional (BasePolicy no-ops)
```

- **Batched-first.** One `act` answers a whole round of games (one forward for a net; one budgeted
  search over all roots). Stateless policies ignore `env_indices` (`**kwargs`); stateful ones route
  per-game state by it.
- **Both modes always.** The evaluated policy is run `greedy` (argmax — the headline) *and* `sample`
  (the honest learning-signal curve). A collapsed policy reads greedy≈0 while sampled shows signal —
  keep both (the MuZero lesson).

Shipped adapters: `RandomPolicy` (torch-free baseline / opponent), `SB3Policy` (MaskablePPO
checkpoint or live model), `SearchPolicy` (documented stub).

---

## Pieces

| Thing | What |
| --- | --- |
| `Arena(deck).play / .evaluate` | N seeded games policy-A vs policy-B on a batched lockstep pump → `EvalResult` (win_rate + Wilson CI, wins/losses/draws, avg_turns, `stats` ratios, `end_reasons`, deck `analyzers`). Deterministic given `(seed, n_games, batch_size)`; a `RandomPolicy` opponent is bit-identical to the legacy `MtgEnv(opponent="random")` path. |
| `Ladder` | Framework-managed %-trained snapshots (10/25/50/75%) via algorithm-supplied `snapshot_fn`/`load_policy_fn`; logs `ladder/winrate_vs_NNpct`. |
| `evaluate_checkpoint(policy, step, run_dir, *, deck, …)` | Plain-function hook any custom loop calls: greedy+sampled battery → canonical TB scalars + `<run>/evalkit/eval_stepNNN.json` + one replay. |
| `EvalkitCallback` | Drop-in SB3 callback (replaces `SelfPlayEval`+`LadderEval`+`ReplayCheckpoint`), used by `selfplay_train.py`. Tag-set-identical. |
| `python -m mtgenv_gym.evalkit …` | Offline CLI / backfill: load a checkpoint via a named adapter, eval, append scalars+JSON+replay to a run. |
| analyzers | Per-deck judgment (e.g. `swine` chump/gang-at-high-life) run + logged automatically when the deck matches. |

---

## Canonical TensorBoard schema (backward-compatible with the 2.x PPO runs)

```
selfplay/winrate_vs_random        selfplay/winrate_vs_random_sampled
selfplay/winrate_vs_initial       selfplay/winrate_vs_initial_sampled
ladder/winrate_vs_10pct … _75pct
stats/{attack_rate, productive_rate, block_rate, cast_rate, playland_rate, block_double_rate}
game/turns_mean   game/end_<reason>
<deck>/<analyzer-scalar>          # e.g. swine/chump_rate_swine_hi
```

The `stats/*` ratios reuse `mtgenv_gym.tracked_stats` **exactly** (same registry) — the metric an
eval logs is the identical definition the training rollout logs, no drift. In training,
`stats/*`+`game/*` come from the rollout stream (`TrackedStats`/`GameLength`); offline they come from
the eval games.

---

## Extending

**A deck analyzer** — `register_analyzer("mydeck", MyAnalyzer)` where `MyAnalyzer` has
`observe(obs, decision_stats_record)`, `result() -> {tag: value}`, `reset()`. Auto-runs when
`deck == "mydeck"`.

**A CLI adapter** — `register_adapter("muzero", lambda path, device="cpu": MyMuZeroPolicy(path, device))`
so `python -m mtgenv_gym.evalkit --algo muzero …` works without evalkit depending on your stack.

The core (everything except `SB3Policy`/`EvalkitCallback`) is torch/sb3-free — it imports in an
isolated eval venv (e.g. LightZero's) that has `mtg_py` but not stable-baselines3.
