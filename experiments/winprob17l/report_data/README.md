# report_data/ — JSON payloads for the interactive report

Compact, pre-aggregated JSON (no raw rows) for the visualization page. Regenerate any file
with its `src/report_*.py` builder. All win rates are the **data-collector's** win rate
(17lands logs one seat); base rate ≈ 0.554. Sources: SOS PremierDraft `replay_data` (per-turn
board state, 582,914 games) and `game_data` (per-card, same games); card metadata joined
from the local Scryfall sqlite. See `../README.md` for the modeling write-up.

## meta.json  (`src/report_headline.py`)
Provenance for the footer. `{set, set_name, format, source, files{}, date_pulled,
total_games, feature_sample_games, feature_snapshot_rows, card_data_source, note}`.

## headline.json  (`src/report_headline.py`) — full 582,914 games
- `total_games`, `collector_win_rate`, `sample_games` (80k feature sample), `snapshot_rows`.
- `on_play_vs_draw`: `{on_play:{games,win_rate}, on_draw:{games,win_rate}}` (0.580 vs 0.528).
- `game_length`: array of `{num_turns, games, win_rate}`; the last entry is `num_turns:"20+"`.
- `mulligans`: `{kept_all_seven_rate, by_mulligans:[{mulligans, games, win_rate}]}`.

## model.json  (`src/report_model.py`) — 80k-game feature table, by-game 80/20 split
- `base_rate`: `{win_rate, log_loss}` of the constant baseline.
- `metrics`: `{logistic:{auc,log_loss,brier}, mlp:{...}}`.
- `calibration`: `{logistic:[{pred,actual,n}], mlp:[...]}` — reliability curve (10 bins).
- `auc_by_turn`: `{logistic:[{bucket,n,win_rate,auc}]}` — AUC by turn range.
- `coefficients`: `[{feature, coef, raw_sd}]` — STANDARDIZED logistic coefs, sorted by |coef|
  (coef = log-odds shift per +1 SD; the "what matters" table).
- `phi_audit`: `{current_phi_auc, ceiling_auc_23feat, spearman_rho_vs_data,
  terms:[{term, hand_weight, data_weight, std_coef}], note}` — the gym's current tanh Φ vs
  the data (0.710 AUC vs 0.739 ceiling, ρ=0.965; terms = dcards/dpower/dlife).

## features.json  (`src/report_features.py`) — the "what actually matters" visual
Win rate vs binned feature *differential* (me − opp) at turn checkpoints 5/8/11 (`turn` =
ply = MTG turn number). `{checkpoints, turn_semantics, rows_per_checkpoint, min_bin_n,
features, defs}`. `features` = `{lands_diff, hand_diff, creature_diff, life_diff}`, each a
map `"<checkpoint>" -> [{bin, x, win_rate, n}]` (`x` = numeric bin center for plotting;
count diffs clamp to ±4 with `<=`/`>=` end bins; life diff in width-5 buckets). Bins with
n < `min_bin_n` (80) are dropped.

## cards.json  (`src/report_cards.py`) — per-card rankings, full 582,914 games
`{n_games, min_gp_filter, metric_defs, cards:[...]}`. One entry per SOS card with maindeck
count ≥ `min_gp_filter` (200), sorted by `gih_wr` desc. Each card:
- `name, mana_cost, cmc, colors` (WUBRG string), `rarity, type` (front face).
- `gih_wr` + `n_gih` — win rate in games the card was ever in hand (opening_hand+drawn+tutored).
- `oh_wr` + `n_oh` — win rate with the card in the opening hand.
- `gp_wr` + `n_gp` — win rate in games with the card in the maindeck (games played).
`n_*` are sample sizes for confidence gating. DFC/split cards are listed by front-face name
(matched to Scryfall's "Front // Back"). Basic lands are included — filter on `type` if the
viz wants to exclude them (their WR ≈ base rate and is not meaningful).
