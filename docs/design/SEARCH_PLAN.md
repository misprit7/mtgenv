# SEARCH_PLAN — explicit search over the rules engine

*Written 2026-07-09 (overnight). Status: **greenlit by the user** ("Okay lets implement explicit
search and use it"), reversing the 2026-07-06 no-real-state-search stance. Companion history:
the treesearch-replacement survey's Gumbel-AZ-over-the-real-sim recommendation is now operative.*

## 0. Why explicit (engine) search, not a learned model

| Consideration | Engine search | Learned-model (MuZero-family) |
|---|---|---|
| Model fidelity | perfect, incl. rare combat corner cases | least faithful exactly on rare pivotal states (trained on the passive self-play distribution) |
| Cost of the model | free — we own a fast, deterministic, clonable sim | the dominant research risk (EZ worked on heralds; StochMZ/UniZero collapsed; low-sim Gumbel fragile) |
| Hidden information | needs determinization (see §3) | handled by construction, but values still learned from self-play data |
| Sim speed | μs/decision engine steps | GPU latent steps (batched) |

The usual reason to learn a model — the real env is slow or unavailable — does not apply here.
Low-sim-count search with a *perfect* model is the intended regime (muzero2's low-sim fragility
was a learned-model artifact).

## 1. The core primitive: `PyGame.fork()` (clone by decision-log replay)

The Session fiber is deliberately not cloneable (RESUMABLE_ENGINE.md), but the engine is
**deterministic given (decks, seed, decision sequence)** — the M3 fingerprint gate proved
byte-identical trajectories. So a clone is a *replay*:

- `PyGame` records every applied factored action (`actions_log: Vec<u32>`, appended in `step`).
- `fork()` constructs a fresh session with the same decks + seed and re-applies the log
  (Interaction/apply path only — **no observation encoding during replay**), yielding an
  independent game at the identical decision point.
- Cost: O(actions so far) engine steps ≈ ms at mid-game. Search forks once per candidate at the
  root; rollouts step forward without further forking.

This is deliberately the dumb-but-honest primitive. If fork cost ever binds (deep searches,
long games), the upgrade is engine-side state snapshotting — but not before profiling says so.

## 2. v0 search agent: Monte-Carlo rollout search at combat decisions

**Goal (overnight item 3): a ladder agent that uses the engine for lookahead.** Not yet a
training-time teacher — first prove the machinery and measure the play-strength delta.

- At a **combat decision** (`DeclareAttackers` / `DeclareBlockers` sub-steps — the decisions the
  4.x/5.x campaign showed are judged worst), enumerate the legal candidates (the mask; ≤ ~10 in
  these pools). For each candidate: `fork()`, apply it, then roll the game to terminal with the
  base policy net playing **both seats** (sampled mode, temperature 1), `n_rollouts` times.
  Q̂(a) = mean outcome. Play argmax.
- Every other decision: play the base policy directly (greedy).
- Base net: 5.0-attn-v3 (the v3 reference). Agent name: `5.0+mcs<R>` (R = rollouts/candidate).
- Cost envelope (swine): ~10–15 searched decisions/game × ≤10 candidates × R rollouts ×
  ~40 remaining decisions ≈ 1–2 s/game at R=2 with batched forwards — a 1,000-game rating add
  runs in well under an hour.

Why MC rollouts and not value-net leaves for v0: the value net is trained on the passive
distribution — exactly the thing we distrust; full rollouts to the *true* terminal outcome need
no trust. (Gumbel/PUCT with value bootstrapping is the v2 upgrade once distillation needs it.)

## 3. Hidden information — the honesty ledger

`fork()` replays the same seed ⇒ the fork contains the **true** hidden state (opponent's actual
hand, real library order). A v0 search therefore plays with a small oracle advantage (it "knows"
future draws through rollout outcomes).

- **v0 (accepted, documented):** in the current pools the leak is small (no combat tricks; hands
  are creatures+lands) — but the agent is labeled `oracle` in its ladder notes, and **oracle
  search must NEVER be used to generate training targets** (strategy fusion / learned delusion).
- **v1 (determinization):** `fork(reshuffle_hidden=seed)` — engine support to re-deal all
  hidden-to-the-searcher zones (opponent hand + both libraries) uniformly consistent with the
  searcher's information set (counts + known cards). Search K determinizations, average Q̂.
  This is the version distillation is allowed to learn from.

## 4. v2+: search as the training-time teacher (the actual point)

1. **Distillation (AZ-style expert iteration):** run generation with determinized search on;
   train the policy head toward the search-improved action distribution (cross-entropy on
   visit/Q-derived targets) and the value head toward outcomes. Search amplifies, the net
   distills, repeat — the automatic teacher that replaces hand-written punishers.
2. **Gumbel at low sims** replaces exhaustive-candidate MC when candidate sets grow
   (SOS targeting): sample-without-replacement candidates, sequential halving, completed-Q.
3. **Where it runs:** the search loop moves Rust-side (batched leaf/rollout net evals via one
   PyO3 crossing per batch — the pump lesson) when Python orchestration binds; v0 is
   Python-orchestrated because eval-time cost is tolerable.

## 5. Staged milestones

| Stage | Deliverable | Exit |
|---|---|---|
| S0 (tonight) | `PyGame.fork()` + `actions_log`; Rust tests (fork mid-combat ⇒ identical obs/mask; diverge after different actions) | workspace green |
| S1 (tonight) | `python/search_agent.py` MC-rollout `SearchPolicy` + rate_agent `--kind search`; `5.0+mcs2` on the v3 swine ladder | Elo + combat metrics vs 5.0; free-attack/lone-bear behavior of the SAME net ± search |
| S2 | determinized fork (`reshuffle_hidden`) | oracle-vs-determinized Elo gap measured |
| S3 | distillation loop (generation with search → policy/value targets) | a trained-with-search agent beats the same recipe without search on the ladder |
| S4 | Rust search loop + Gumbel (scale to SOS action sets) | search overhead ≤2× training wall clock |

**S1's hypothesis test, stated up front:** if one-ply-plus-rollouts fixes free attacks and
lone-bear discipline *with the same net*, the deficit is judgment-at-decision-time (search
buys it directly and distillation will transfer it); if search doesn't help, the value signal
in rollouts is too noisy and S3 needs value-target work first.
