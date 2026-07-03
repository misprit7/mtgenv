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

- **2026-07-03 — Phase 0 GATE: GO.** SOS replay + game data verified present on 17lands
  S3 with the required per-turn end-of-turn board state, and board composition is
  recoverable (see below). Proceeding to Phase 1 (download slice + logistic baseline).

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
