"""Audit the gym's current hand-tuned tanh Φ against what the 17lands data actually says.

The current Φ (python/mtgenv_gym/batched_selfplay.py::_phi_batch) is:
    dlife  = my_life - opp_life
    dcards = (my_hand + my_bf) - (opp_hand + opp_bf)       # bf = ALL permanents incl lands
    dpower = sum(my creature power) - sum(opp creature power)
    Φ = 0.5*tanh(dcards/4) + 0.3*tanh(dpower/6) + 0.2*tanh(dlife/10)

Every input maps onto our 17lands features, so we can evaluate the *exact* current Φ on the
same held-out states and compare it, term by term, to the data-optimal weighting.

Run: python src/audit_current_phi.py
"""
from pathlib import Path
import numpy as np
import pandas as pd
from sklearn.linear_model import LogisticRegression
from sklearn.preprocessing import StandardScaler
from sklearn.metrics import roc_auc_score
from scipy.stats import spearmanr

from train_baseline import FEATURES, by_game_split

HERE = Path(__file__).resolve().parent
FEATS = HERE.parent / "data" / "features_SOS_PremierDraft.parquet"


def current_phi(df):
    """The gym's exact hand-tuned Φ, reconstructed from our features."""
    my_bf = df["my_lands"] + df["my_creatures"] + df["my_noncreatures"]
    opp_bf = df["opp_lands"] + df["opp_creatures"] + df["opp_noncreatures"]
    dcards = (df["my_hand"] + my_bf) - (df["opp_hand"] + opp_bf)
    dpower = df["my_pow_sum"] - df["opp_pow_sum"]
    dlife = df["my_life"] - df["opp_life"]
    return (0.5 * np.tanh(dcards / 4.0)
            + 0.3 * np.tanh(dpower / 6.0)
            + 0.2 * np.tanh(dlife / 10.0)).values


def diff_feats(df):
    """Signed differential versions of the aggregate signals (me - opp)."""
    return pd.DataFrame({
        "dcards": (df["my_hand"] + df["my_lands"] + df["my_creatures"] + df["my_noncreatures"])
                  - (df["opp_hand"] + df["opp_lands"] + df["opp_creatures"] + df["opp_noncreatures"]),
        "dpower": df["my_pow_sum"] - df["opp_pow_sum"],
        "dlife": df["my_life"] - df["opp_life"],
        "dlands": df["my_lands"] - df["opp_lands"],
        "dhand": df["my_hand"] - df["opp_hand"],
        "dcreat": df["my_creatures"] - df["opp_creatures"],
    })


def fit_auc(Xtr, ytr, Xte, yte, cols):
    sc = StandardScaler().fit(Xtr[cols].values)
    clf = LogisticRegression(max_iter=2000).fit(sc.transform(Xtr[cols].values), ytr)
    p = clf.predict_proba(sc.transform(Xte[cols].values))[:, 1]
    return roc_auc_score(yte, p), dict(zip(cols, clf.coef_[0]))


def main():
    df = pd.read_parquet(FEATS)
    tr, te = by_game_split(df)
    ytr, yte = df.loc[tr, "won"].values, df.loc[te, "won"].values

    # --- 1. Current Φ as a raw win predictor (no refit — the actual hand-tuned function) ---
    phi_te = current_phi(df.loc[te])
    auc_cur = roc_auc_score(yte, phi_te)
    print("=== 1. CURRENT hand-tuned Φ as a win predictor (test set) ===")
    print(f"  current-Φ AUC        : {auc_cur:.4f}")
    print(f"  full 23-feat logistic: 0.7392   (from train_baseline)")
    print(f"  --> the current Φ leaves {0.7392 - auc_cur:+.4f} AUC on the table vs the generic-feature ceiling")

    # --- 2. Data-optimal weighting of the SAME three Φ terms ---
    Dtr, Dte = diff_feats(df.loc[tr]), diff_feats(df.loc[te])
    three = ["dcards", "dpower", "dlife"]
    sc = StandardScaler().fit(Dtr[three].values)
    clf = LogisticRegression(max_iter=2000).fit(sc.transform(Dtr[three].values), ytr)
    coef = clf.coef_[0]
    w_data = np.abs(coef) / np.abs(coef).sum()
    w_hand = np.array([0.5, 0.3, 0.2])
    print("\n=== 2. Data-optimal weights for the current Φ's 3 terms (std. logistic) ===")
    print(f"  {'term':<8}{'hand-tuned':>12}{'data-optimal':>14}{'std-coef':>10}")
    for i, t in enumerate(three):
        print(f"  {t:<8}{w_hand[i]:>12.2f}{w_data[i]:>14.2f}{coef[i]:>10.3f}")
    print("  (data-optimal = |std coef| normalized to sum 1; same-sign as hand-tuned = correct direction)")

    # --- 3. Nested-model AUCs: value of a lands term, cost of the power term ---
    print("\n=== 3. What the weighting misses/wastes (test AUC of small logistic models) ===")
    combos = [
        ("current Φ inputs      [dcards,dpower,dlife]", ["dcards", "dpower", "dlife"]),
        ("drop power term       [dcards,dlife]       ", ["dcards", "dlife"]),
        ("add distinct lands    [dcards,dpower,dlife,dlands]", ["dcards", "dpower", "dlife", "dlands"]),
        ("counts split+lands    [dhand,dlands,dcreat,dlife]", ["dhand", "dlands", "dcreat", "dlife"]),
    ]
    base = None
    for name, cols in combos:
        auc, _ = fit_auc(Dtr, ytr, Dte, yte, cols)
        if base is None:
            base = auc
        print(f"  {name:<52} AUC {auc:.4f}  ({auc-base:+.4f} vs current-inputs)")

    # --- 4. Rank agreement between current Φ and a data-fit Φ on the 3 terms ---
    p_data = clf.predict_proba(sc.transform(Dte[three].values))[:, 1]
    phi_data = 2 * p_data - 1
    rho = spearmanr(phi_te, phi_data).statistic
    print("\n=== 4. Rank agreement: current Φ vs data-refit Φ (same 3 terms) ===")
    print(f"  Spearman ρ = {rho:.4f}  (how much the *shape* already agrees despite the mis-weighting)")


if __name__ == "__main__":
    main()
