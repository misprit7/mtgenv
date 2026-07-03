"""Build report_data/model.json — everything about the win-prob models for the report:
logistic + MLP metrics, calibration curves (both), AUC-by-turn, the standardized
coefficient table, and the Φ-audit numbers. Reuses the Phase-1/2 modules verbatim so the
JSON matches the committed results.
"""
import json
from pathlib import Path
import numpy as np
import pandas as pd
import torch
import torch.nn as nn
from sklearn.linear_model import LogisticRegression
from sklearn.preprocessing import StandardScaler
from sklearn.metrics import roc_auc_score, log_loss, brier_score_loss
from scipy.stats import spearmanr

from train_baseline import FEATURES, by_game_split, reliability_table
from train_mlp import MLP
from audit_current_phi import current_phi, diff_feats

HERE = Path(__file__).resolve().parent
FEATS = HERE.parent / "data" / "features_SOS_PremierDraft.parquet"
OUT = HERE.parent / "report_data" / "model.json"

torch.manual_seed(17)


def calib_points(y, p, bins=10):
    return [{"pred": round(mp, 4), "actual": round(fp, 4), "n": int(n)}
            for _, n, mp, fp in reliability_table(y, p, bins=bins) if n > 0 and mp == mp]


def auc_by_turn(turn, y, p):
    df = pd.DataFrame({"turn": turn, "y": y, "p": p})
    out = []
    for lo, hi in [(1, 4), (5, 8), (9, 12), (13, 18), (19, 60)]:
        m = (df["turn"] >= lo) & (df["turn"] <= hi)
        if m.sum() < 50 or df.loc[m, "y"].nunique() < 2:
            continue
        out.append({"bucket": f"{lo}-{hi}", "n": int(m.sum()),
                    "win_rate": round(float(df.loc[m, "y"].mean()), 4),
                    "auc": round(float(roc_auc_score(df.loc[m, "y"], df.loc[m, "p"])), 4)})
    return out


def main():
    df = pd.read_parquet(FEATS)
    tr, te = by_game_split(df)
    Xtr, Xte = df.loc[tr, FEATURES].values, df.loc[te, FEATURES].values
    ytr, yte = df.loc[tr, "won"].values, df.loc[te, "won"].values
    turn_te = df.loc[te, "turn"].values

    sc = StandardScaler().fit(Xtr)
    Xtr_s, Xte_s = sc.transform(Xtr), sc.transform(Xte)

    # --- logistic ---
    clf = LogisticRegression(max_iter=3000).fit(Xtr_s, ytr)
    pl = clf.predict_proba(Xte_s)[:, 1]
    base = ytr.mean()
    log_metrics = {
        "auc": round(float(roc_auc_score(yte, pl)), 4),
        "log_loss": round(float(log_loss(yte, pl)), 4),
        "brier": round(float(brier_score_loss(yte, pl)), 4),
    }
    coefs = [{"feature": f, "coef": round(float(c), 4), "raw_sd": round(float(s), 3)}
             for f, c, s in zip(FEATURES, clf.coef_[0], Xtr.std(0))]
    coefs.sort(key=lambda d: -abs(d["coef"]))

    # --- MLP (CPU) ---
    Xtr_t = torch.tensor(Xtr_s, dtype=torch.float32)
    Xte_t = torch.tensor(Xte_s, dtype=torch.float32)
    ytr_t = torch.tensor(ytr.astype("float32"))
    model = MLP(len(FEATURES))
    opt = torch.optim.Adam(model.parameters(), lr=1e-3, weight_decay=1e-5)
    lossf = nn.BCEWithLogitsLoss()
    n, bs = Xtr_t.shape[0], 8192
    for _ in range(15):
        model.train()
        perm = torch.randperm(n)
        for i in range(0, n, bs):
            idx = perm[i:i + bs]
            opt.zero_grad()
            lossf(model(Xtr_t[idx]), ytr_t[idx]).backward()
            opt.step()
    model.eval()
    with torch.no_grad():
        pm = torch.sigmoid(model(Xte_t)).numpy()
    mlp_metrics = {
        "auc": round(float(roc_auc_score(yte, pm)), 4),
        "log_loss": round(float(log_loss(yte, pm)), 4),
        "brier": round(float(brier_score_loss(yte, pm)), 4),
    }

    # --- Φ audit (reuse audit module logic) ---
    Dtr, Dte = diff_feats(df.loc[tr]), diff_feats(df.loc[te])
    three = ["dcards", "dpower", "dlife"]
    scd = StandardScaler().fit(Dtr[three].values)
    clfd = LogisticRegression(max_iter=2000).fit(scd.transform(Dtr[three].values), ytr)
    coef3 = clfd.coef_[0]
    wdata = np.abs(coef3) / np.abs(coef3).sum()
    phi_cur = current_phi(df.loc[te])
    pdata = clfd.predict_proba(scd.transform(Dte[three].values))[:, 1]
    rho = float(spearmanr(phi_cur, 2 * pdata - 1).statistic)
    hand_w = {"dcards": 0.5, "dpower": 0.3, "dlife": 0.2}
    phi_audit = {
        "current_phi_auc": round(float(roc_auc_score(yte, phi_cur)), 4),
        "ceiling_auc_23feat": log_metrics["auc"],
        "spearman_rho_vs_data": round(rho, 4),
        "terms": [{"term": t, "hand_weight": hand_w[t],
                   "data_weight": round(float(wdata[i]), 3),
                   "std_coef": round(float(coef3[i]), 3)}
                  for i, t in enumerate(three)],
        "note": "current gym Φ = 0.5·tanh(dcards/4)+0.3·tanh(dpower/6)+0.2·tanh(dlife/10); "
                "dcards lumps hand+all permanents (incl lands). Indicated tweak: power→life reweight.",
    }

    payload = {
        "base_rate": {"win_rate": round(float(base), 4),
                      "log_loss": round(float(log_loss(yte, np.full_like(pl, base))), 4)},
        "metrics": {"logistic": log_metrics, "mlp": mlp_metrics},
        "calibration": {"logistic": calib_points(yte, pl), "mlp": calib_points(yte, pm)},
        "auc_by_turn": {"logistic": auc_by_turn(turn_te, yte, pl)},
        "coefficients": coefs,
        "phi_audit": phi_audit,
        "n_train_rows": int(tr.sum()), "n_test_rows": int(te.sum()),
    }
    OUT.write_text(json.dumps(payload, indent=2))
    print(f"model.json: logistic AUC {log_metrics['auc']}, MLP AUC {mlp_metrics['auc']}, "
          f"phi-audit AUC {phi_audit['current_phi_auc']} (rho {phi_audit['spearman_rho_vs_data']})")


if __name__ == "__main__":
    main()
