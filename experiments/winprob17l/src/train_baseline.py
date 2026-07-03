"""Logistic-regression baseline for P(win | generic board state) on the SOS feature table.

- Train/test split is BY GAME (rows within a game share the label and are correlated).
- Reports: ROC-AUC, log-loss, Brier, a calibration curve (reliability table), and the
  standardized logistic COEFFICIENTS (the data-derived "how much do cards vs board vs life
  actually matter" answer).
- Also reports metrics stratified by turn bucket, since early-game states are near
  50/50 and late-game states near-deterministic.

Run: python src/train_baseline.py
"""
import sys
from pathlib import Path
import numpy as np
import pandas as pd
from sklearn.linear_model import LogisticRegression
from sklearn.preprocessing import StandardScaler
from sklearn.metrics import roc_auc_score, log_loss, brier_score_loss
from sklearn.calibration import calibration_curve

HERE = Path(__file__).resolve().parent
DATA = HERE.parent / "data"
FEATS = DATA / "features_SOS_PremierDraft.parquet"

# generic feature columns (must match parse_replay.py / README feature spec)
FEATURES = [
    "turn", "my_turn", "on_play",
    "my_life", "opp_life",
    "my_hand", "opp_hand",
    "my_lands", "opp_lands",
    "my_creatures", "opp_creatures",
    "my_noncreatures", "opp_noncreatures",
    "my_pow_sum", "opp_pow_sum",
    "my_tou_sum", "opp_tou_sum",
    "my_pow_max", "opp_pow_max",
    "my_cmc_sum", "opp_cmc_sum",
    "my_cmc_mean", "opp_cmc_mean",
]


def by_game_split(df, test_frac=0.2, seed=17):
    games = df["game_id"].unique()
    rng = np.random.default_rng(seed)
    rng.shuffle(games)
    n_test = int(len(games) * test_frac)
    test_games = set(games[:n_test].tolist())
    is_test = df["game_id"].isin(test_games).values
    return ~is_test, is_test


def reliability_table(y, p, bins=10):
    frac_pos, mean_pred = calibration_curve(y, p, n_bins=bins, strategy="uniform")
    # counts per bin
    edges = np.linspace(0, 1, bins + 1)
    idx = np.clip(np.digitize(p, edges) - 1, 0, bins - 1)
    counts = np.bincount(idx, minlength=bins)
    rows = []
    j = 0
    for b in range(bins):
        lo, hi = edges[b], edges[b + 1]
        cnt = counts[b]
        if cnt == 0:
            rows.append((f"[{lo:.1f},{hi:.1f})", cnt, float("nan"), float("nan")))
            continue
        rows.append((f"[{lo:.1f},{hi:.1f})", int(cnt), mean_pred[j], frac_pos[j]))
        j += 1
    return rows


def main():
    df = pd.read_parquet(FEATS)
    print(f"loaded {len(df):,} rows over {df['game_id'].nunique():,} games; "
          f"base win rate {df['won'].mean():.4f}")

    tr, te = by_game_split(df)
    Xtr, Xte = df.loc[tr, FEATURES].values, df.loc[te, FEATURES].values
    ytr, yte = df.loc[tr, "won"].values, df.loc[te, "won"].values
    print(f"train rows {tr.sum():,} | test rows {te.sum():,} "
          f"(games disjoint; test frac by game = 0.2)")

    scaler = StandardScaler().fit(Xtr)
    Xtr_s, Xte_s = scaler.transform(Xtr), scaler.transform(Xte)

    clf = LogisticRegression(max_iter=2000, C=1.0)
    clf.fit(Xtr_s, ytr)
    p = clf.predict_proba(Xte_s)[:, 1]

    auc = roc_auc_score(yte, p)
    ll = log_loss(yte, p)
    brier = brier_score_loss(yte, p)
    # baseline log-loss of predicting the constant base rate
    base = ytr.mean()
    ll_base = log_loss(yte, np.full_like(p, base))
    print("\n=== TEST METRICS (logistic, by-game split) ===")
    print(f"  ROC-AUC   : {auc:.4f}")
    print(f"  log-loss  : {ll:.4f}  (constant-base-rate baseline {ll_base:.4f})")
    print(f"  Brier     : {brier:.4f}")

    print("\n=== CALIBRATION (reliability table, 10 uniform bins) ===")
    print(f"  {'bin':<12}{'n':>8}{'mean_pred':>12}{'frac_win':>12}")
    for name, cnt, mp, fp in reliability_table(yte, p, bins=10):
        mps = f"{mp:.3f}" if mp == mp else "  -"
        fps = f"{fp:.3f}" if fp == fp else "  -"
        print(f"  {name:<12}{cnt:>8}{mps:>12}{fps:>12}")

    print("\n=== STANDARDIZED LOGISTIC COEFFICIENTS (sorted by |coef|) ===")
    print("  (feature standardized to unit variance; coef = log-odds shift per +1 SD)")
    coefs = sorted(zip(FEATURES, clf.coef_[0]), key=lambda kv: -abs(kv[1]))
    print(f"  {'feature':<18}{'coef':>10}{'  (SD of raw feature)':>0}")
    sds = Xtr.std(axis=0)
    sd_map = dict(zip(FEATURES, sds))
    for f, c in coefs:
        print(f"  {f:<18}{c:>10.4f}    raw_sd={sd_map[f]:.2f}")
    print(f"  {'intercept':<18}{clf.intercept_[0]:>10.4f}")

    print("\n=== AUC BY TURN BUCKET (test) ===")
    tdf = df.loc[te, ["turn", "won"]].copy()
    tdf["p"] = p
    buckets = [(1, 4), (5, 8), (9, 12), (13, 18), (19, 60)]
    print(f"  {'turns':<10}{'n':>8}{'win_rate':>10}{'auc':>8}")
    for lo, hi in buckets:
        m = (tdf["turn"] >= lo) & (tdf["turn"] <= hi)
        if m.sum() < 50 or tdf.loc[m, "won"].nunique() < 2:
            print(f"  {f'{lo}-{hi}':<10}{int(m.sum()):>8}{'-':>10}{'-':>8}")
            continue
        a = roc_auc_score(tdf.loc[m, "won"], tdf.loc[m, "p"])
        print(f"  {f'{lo}-{hi}':<10}{int(m.sum()):>8}{tdf.loc[m,'won'].mean():>10.3f}{a:>8.3f}")


if __name__ == "__main__":
    main()
