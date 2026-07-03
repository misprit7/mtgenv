# winprob17l — supervised win-probability shaping potential from 17lands data

**SPECULATIVE research track.** Goal: train a supervised win-probability model
`P(win | generic board state)` on real human 17lands gameplay for *Secrets of
Strixhaven* (`SOS`) limited, using **only generic, format-agnostic features** (life
totals, per-creature P/T/CMC aggregates, counts). If it calibrates well, expose it later
(flag-gated) as the RL shaping potential Φ via **Φ = 2·P(win) − 1 ∈ [−1, 1]**, replacing
the current hand-tuned tanh heuristic with a lower-noise, data-derived one.

Honest negative results are a success state for this track — the point is to find out
whether this works, not to make it work.

## Isolation

All code and data for this experiment live under `experiments/winprob17l/`. This phase
makes **no changes** to `crates/`, `python/mtgenv_gym`, or training code. The only eventual
integration point is a flag-gated adapter exposing `phi(features) -> [-1, 1]`, and that
comes later, after results justify it. `data/` is gitignored.

## Status

- **2026-07-03 — Phase 1 done: baseline trained.** Logistic + small MLP on 80k games /
  1.44M turn-snapshots. Both are **very well calibrated** (the property that matters for a
  potential). Logistic AUC 0.739 / log-loss 0.590; MLP AUC 0.764 / log-loss 0.564 — a
  modest, real nonlinear gain. Verdict: **the idea is viable in-distribution**; the open
  question is transfer to our tiny-pool self-play (untested — see Findings §Distribution
  shift). Full write-up below; raw output in `results/`.
- **2026-07-03 — Phase 0 GATE: GO.** SOS replay + game data verified present on 17lands
  S3 with the required per-turn end-of-turn board state, and board composition is
  recoverable (see below).

## Phase 1 findings

**Dataset.** `replay_data_public.SOS.PremierDraft` has **582,914 games** (collector win
rate 0.554, avg 9.25 turns). We randomly sampled **80,000 games** (seed 17) → **1,437,392
turn-snapshot rows** (one per end-of-turn state, both players' turns, collector
perspective). Split **by game** 80/20 (train 1.15M rows / test 288k rows, disjoint games).

**Model comparison (test set, by-game split):**

| model                 | ROC-AUC | log-loss | Brier | calibration        |
|-----------------------|--------:|---------:|------:|--------------------|
| constant base-rate    |   0.500 |   0.689  |  —    | —                  |
| **logistic (23 feat)**|   0.739 |   0.590  | 0.203 | excellent (≤0.02)  |
| **MLP 64-32**         |   0.764 |   0.564  | 0.194 | excellent (≤0.01)  |

Both reliability tables track the diagonal within ~1–2% in every decile (e.g. logistic:
pred 0.85 → actual 0.857; MLP: pred 0.95 → 0.957). **Calibration — the thing that makes a
usable Φ — is already excellent, even for the linear model.** The MLP's edge is a modest
+0.025 AUC / −0.026 log-loss from nonlinear interactions.

**Coefficient table — the data-derived answer to "how much do cards vs board vs life
actually matter."** Standardized logistic coefficients (log-odds shift per +1 SD of the
feature; `my_*` = the perspective player, `opp_*` = opponent). Sorted by magnitude:

| rank | feature (me / opp)        | coef (me) | coef (opp) | reading                              |
|-----:|---------------------------|----------:|-----------:|--------------------------------------|
| 1 | **lands in play**            |   +0.84   |   −0.80    | mana development / tempo dominates   |
| 2 | **cards in hand**            |   +0.68   |   −0.69    | card advantage is a close #2         |
| 3 | **creature count**           |   +0.57   |   −0.58    | bodies on board, #3                  |
| 4 | **life total**               |   +0.42   |   −0.40    | life matters *less* than the above 3 |
| 5 | non-creature permanents      |   +0.27   |   −0.26    |                                      |
| 6 | creature CMC-sum             |   +0.24   |   −0.23    | board investment proxy               |
| 7 | creature toughness-sum       |   +0.20   |   −0.19    |                                      |
| 8 | creature power-sum           |   −0.15   |   +0.13    | **sign-flipped (collinear noise)**   |
|   | creature power-max, cmc-mean | 0.04–0.08 | 0.05–0.08  | negligible                           |
|   | `my_turn` −0.36, `on_play` +0.05, `turn` −0.03, intercept +0.25 |||          |

**The headline for the lead:** it's **counts, not card stats.** Land count, hand size, and
creature count carry almost all the signal; life is a clear but secondary factor; the
per-creature P/T/CMC aggregates add very little on top (and `power-sum` even sign-flips —
classic multicollinearity once creature *count* is in the model). Concretely: the current
hand-tuned tanh Φ leaning on life/board-power may be **over-weighting life and P/T and
under-weighting mana development and card advantage** relative to what real games say.

**AUC by turn bucket (test)** — discrimination grows as the game develops, exactly the
honest profile you want from a potential:

| turns | n | win rate | AUC |
|------:|--:|---------:|----:|
| 1–4   | 64.0k | 0.550 | 0.576 |
| 5–8   | 63.8k | 0.550 | 0.667 |
| 9–12  | 60.9k | 0.549 | 0.758 |
| 13–18 | 64.6k | 0.543 | 0.827 |
| 19+   | 34.7k | 0.538 | 0.841 |

Early states (turns 1–4) are near-uninformative (AUC 0.58 — the game genuinely isn't
decided); late states are near-deterministic (0.84). A shaping potential built on this will
be nearly flat early and sharpen late — sensible.

### Distribution shift (the real risk — UNTESTED)

Everything above is **in-distribution** (human, full 271-card SOS pool, human policy). Our
RL target is **tiny-pool self-play** with a learning policy. The bet is that the top
signals — land/hand/creature counts and life — are format-agnostic enough to transfer.
Reasons for cautious optimism: (a) the model leans on generic *counts*, not card identity,
which is exactly what a small pool still has; (b) potential-based shaping is policy-
invariant (Ng et al. 1999) — a miscalibrated Φ only changes *learning speed*, never the
optimal policy, so the downside is bounded. Reasons for caution: (a) our pool's card-power
distribution differs, so the same "3 creatures / 5 lands" state maps to a different true
P(win); (b) the MLP's interaction terms are the most likely to *not* transfer — **prefer
the logistic Φ for its graceful linear extrapolation off-distribution.** This is the one
thing results so far cannot settle; it needs an actual A/B in the gym (see below).

### Proposed integration design (NOT implemented this phase)

- **Adapter shape.** A pure function `phi(features: [f32; 23]) -> f32 in [-1, 1]`, computed
  as `phi = 2*sigmoid(w·standardize(x)) - 1` from a frozen `(mean, sd, weights, bias)`
  blob exported by `train_baseline.py`. The 23 features are exactly the generic vector in
  the spec, all computable from the engine's `PlayerView` (life, four counts per side, four
  creature aggregates per side, turn/on_play). No card identity, no set data, no sqlite at
  inference — the P/T/CMC aggregates come from the engine's own card characteristics.
- **Turn-boundary-only shaping (recommended).** The training data is *end-of-turn* states,
  so Φ is only meaningfully defined there. Apply the shaping term
  `F = γ·Φ(s') − Φ(s)` **only at turn boundaries**, not per intra-turn decision (Φ is
  undefined/untrained on mid-combat states, and per-step shaping on an ill-defined Φ adds
  noise). This keeps the potential-based-shaping invariance intact (any Φ on the
  turn-boundary MDP is valid).
- **Flag-gated.** One config flag selects `phi_source ∈ {tanh_heuristic, winprob17l}`;
  default stays the current heuristic until an A/B shows the learned Φ helps. The frozen
  weight blob lives under `experiments/winprob17l/` and is loaded by the adapter only.
- **Validation before adoption.** A/B two identical training runs (same seed/budget)
  differing only in `phi_source`; compare productive-rate / win-rate learning curves. This
  is the only test that resolves the distribution-shift question.

## Phase 0 findings (the GO/NO-GO gate)

**Data source.** 17lands public datasets are gzip CSVs on a public S3 bucket:
`https://17lands-public.s3.amazonaws.com/analysis_data/{dtype}/{dtype}_public.{SET}.{EVENT}.csv.gz`
where `dtype ∈ {draft_data, game_data, replay_data}`. CC BY 4.0.

**SOS availability (verified via HTTP HEAD, 2026-07-03):**

| dtype        | PremierDraft | TradDraft | Sealed | TradSealed |
|--------------|-------------:|----------:|-------:|-----------:|
| draft_data   |      162 MB  |     16 MB |  403¹  |     403¹   |
| game_data    |       49 MB  |    5.3 MB | 4.0 MB |    1.7 MB  |
| replay_data  |      332 MB  |     35 MB |  25 MB |     10 MB  |

(compressed sizes; ¹ 403 = not published for that format). We use
**`replay_data_public.SOS.PremierDraft.csv.gz`** (332 MB gz, 2679 columns) — it is the
only file with per-turn board state.

**Per-turn structure.** Two column families per turn `N`: `user_turn_N_*` (turns where the
data-collector is active) and `oppo_turn_N_*` (turns where the opponent is active). Both
record end-of-turn (`eot`) state *from the collector's viewpoint*, so we get a state
snapshot at the end of **both** players' turns. The relevant EOT columns per turn:

- `eot_user_life`, `eot_oppo_life` — life totals (float).
- `eot_user_cards_in_hand` — pipe-delimited **arena_id** list (collector's hand is known).
- `eot_oppo_cards_in_hand` — a **count** (float; opponent hand is hidden).
- `eot_{user,oppo}_lands_in_play` — pipe-delimited arena_id lists (both battlefields are
  public info, so opponent's is fully visible).
- `eot_{user,oppo}_creatures_in_play` — pipe-delimited arena_id lists.
- `eot_{user,oppo}_non_creatures_in_play` — pipe-delimited arena_id lists.

Values are **Arena IDs** (e.g. `102521`), *not* card names — cleaner (no comma/quoting
issues). A single-element field may appear as a bare int; multi-element as `a|b|c`; empty
as NaN. Duplicates are allowed (two copies of the same land show twice).

**Board composition IS recoverable.** Arena IDs resolve against the local Scryfall sqlite
(`data/scryfall/cards.sqlite`, column `arena_id`) → `name, type_line, cmc, power,
toughness`. SOS coverage: 281 distinct arena_ids / 271 card names in the sqlite; spot-check
of sample-row IDs resolved 9/9. Split/DFC cards (`Foo // Bar`) resolve to front-face
characteristics, which is what we want.

**Game-level columns:** `won` (bool label), `on_play`, `num_turns`, `num_mulligans`,
`opp_num_mulligans`, `main_colors`, `opp_colors`, `expansion`, `event_type`.

**Reference:** `../../../magician/replay_dtypes.py` (the user's own 17lands project) is a
copy of 17lands' `helper_files/replay_dtypes.py` and documents the full column dtype map.

## Feature spec (SINGLE SOURCE OF TRUTH)

One row per **(game, turn, perspective="user")**. Label = game's `won` (constant within a
game → **split train/test BY GAME**, never by row). All features are **generic** — no card
names, no set-specific columns — so the identical vector is computable later from our
engine's `PlayerView`.

Per row, "me" = the active-turn owner's collector-side fields, "opp" = the other side. For
`user_turn_N` rows, me=user/opp=oppo; the columns are already oriented that way. (Whether we
also emit swapped opponent-perspective rows is a Phase-1 decision; see Risks.)

| feature              | source                                                        |
|----------------------|---------------------------------------------------------------|
| `turn`               | N                                                             |
| `on_play`            | game-level `on_play` (bool→0/1)                               |
| `my_life`,`opp_life` | `eot_user_life`, `eot_oppo_life`                              |
| `my_hand`,`opp_hand` | len(`eot_user_cards_in_hand`), `eot_oppo_cards_in_hand`       |
| `my_lands`,`opp_lands`| count of `eot_{user,oppo}_lands_in_play`                     |
| `my_creatures`,`opp_creatures` | count of `eot_{user,oppo}_creatures_in_play`        |
| `my_noncreatures`,`opp_noncreatures` | count of `eot_{user,oppo}_non_creatures_in_play` |
| `my_pow_sum`,`opp_pow_sum` | Σ power over creatures (via arena_id→Scryfall)          |
| `my_tou_sum`,`opp_tou_sum` | Σ toughness over creatures                              |
| `my_pow_max`,`opp_pow_max` | max power over creatures (0 if none)                    |
| `my_cmc_sum`,`opp_cmc_sum` | Σ cmc over creatures (board investment proxy)           |
| `my_cmc_mean`,`opp_cmc_mean`| mean cmc over creatures (0 if none)                    |

Derived/diff features (life diff, board-power diff, card-advantage) can be added, but the
raw generic set above is the baseline. **No card identity is used** — only counts and
P/T/CMC aggregates. Null power/toughness (e.g. non-creatures that slipped in, `*` P/T)
treated as 0.

## Layout

- `src/` — download + parse + train scripts.
- `data/` — downloaded 17lands CSVs + parsed feature tables (gitignored).
- `README.md` — this file (idea, status, findings, feature spec).

## Risks (honest)

- **Distribution shift (the big one).** Training data is *human full-set-pool* PremierDraft
  (271 distinct SOS cards, human decision policy). Our RL self-play uses a *tiny partial
  pool* and a different (learning) policy. The bet is that generic features (life, board
  power, tempo) transfer even when card identity doesn't; that may be false — 17lands games
  are decided by bombs/synergies our pool lacks. **Bounded downside:** potential-based
  shaping is policy-invariant (Ng et al. 1999) — a miscalibrated Φ slows learning but never
  changes the optimal policy. Worst case it's no better than the current heuristic.
- **Perspective asymmetry.** Data is collector-only. `won` is balanced-ish (draft win rate
  ≈ 50–58%), so both win and loss rows exist, but every row is oriented "me=collector."
  Symmetric-perspective augmentation (swap user/oppo) is possible because both battlefields
  are public and hand features are counts — to confirm in Phase 1.
- **Row correlation.** All turns of a game share one label; must split by game and consider
  turn-stratified evaluation (early-turn states are near-uninformative; late-turn states
  near-deterministic).
