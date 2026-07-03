"""Small MLP for P(win | generic board state), CPU-only. Run ONLY to check whether a
nonlinear model meaningfully beats the logistic baseline. Calibration matters more than
AUC here (the model is a shaping potential). Same by-game split and features as
train_baseline.py.

Run: python src/train_mlp.py
"""
from pathlib import Path
import numpy as np
import pandas as pd
import torch
import torch.nn as nn
from sklearn.preprocessing import StandardScaler
from sklearn.metrics import roc_auc_score, log_loss, brier_score_loss
from sklearn.calibration import calibration_curve

from train_baseline import FEATURES, by_game_split, reliability_table

HERE = Path(__file__).resolve().parent
DATA = HERE.parent / "data"
FEATS = DATA / "features_SOS_PremierDraft.parquet"

torch.manual_seed(17)


class MLP(nn.Module):
    def __init__(self, d, h=(64, 32), p=0.1):
        super().__init__()
        layers, prev = [], d
        for hi in h:
            layers += [nn.Linear(prev, hi), nn.ReLU(), nn.Dropout(p)]
            prev = hi
        layers += [nn.Linear(prev, 1)]
        self.net = nn.Sequential(*layers)

    def forward(self, x):
        return self.net(x).squeeze(-1)


def main():
    torch.set_num_threads(max(1, (torch.get_num_threads() or 4)))
    df = pd.read_parquet(FEATS)
    tr, te = by_game_split(df)
    Xtr = df.loc[tr, FEATURES].values.astype("float32")
    Xte = df.loc[te, FEATURES].values.astype("float32")
    ytr = df.loc[tr, "won"].values.astype("float32")
    yte = df.loc[te, "won"].values.astype("float32")

    sc = StandardScaler().fit(Xtr)
    Xtr = torch.tensor(sc.transform(Xtr), dtype=torch.float32)
    Xte = torch.tensor(sc.transform(Xte), dtype=torch.float32)
    ytr_t = torch.tensor(ytr)

    model = MLP(len(FEATURES))
    opt = torch.optim.Adam(model.parameters(), lr=1e-3, weight_decay=1e-5)
    lossf = nn.BCEWithLogitsLoss()

    n = Xtr.shape[0]
    bs = 8192
    for epoch in range(15):
        model.train()
        perm = torch.randperm(n)
        tot = 0.0
        for i in range(0, n, bs):
            idx = perm[i:i + bs]
            opt.zero_grad()
            out = model(Xtr[idx])
            loss = lossf(out, ytr_t[idx])
            loss.backward()
            opt.step()
            tot += loss.item() * len(idx)
        model.eval()
        with torch.no_grad():
            p = torch.sigmoid(model(Xte)).numpy()
        auc = roc_auc_score(yte, p)
        ll = log_loss(yte, p)
        print(f"epoch {epoch:2d}  train_bce={tot/n:.4f}  test_auc={auc:.4f}  test_logloss={ll:.4f}", flush=True)

    brier = brier_score_loss(yte, p)
    print("\n=== MLP TEST METRICS ===")
    print(f"  ROC-AUC  : {auc:.4f}")
    print(f"  log-loss : {ll:.4f}")
    print(f"  Brier    : {brier:.4f}")
    print("\n=== MLP CALIBRATION (10 uniform bins) ===")
    print(f"  {'bin':<12}{'n':>8}{'mean_pred':>12}{'frac_win':>12}")
    for name, cnt, mp, fp in reliability_table(yte, p, bins=10):
        mps = f"{mp:.3f}" if mp == mp else "  -"
        fps = f"{fp:.3f}" if fp == fp else "  -"
        print(f"  {name:<12}{cnt:>8}{mps:>12}{fps:>12}")


if __name__ == "__main__":
    main()
