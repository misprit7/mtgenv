"""Freeze the logistic win-prob model into a small, dependency-light JSON blob that the
adapter (phi_adapter.py) loads for inference. Fits StandardScaler + LogisticRegression on
ALL sampled rows (more data for the deployable artifact than the 80/20 eval split).

Output: model/phi_logistic.json = {features, mean, scale, coef, intercept, meta}.
Run: python src/export_phi.py
"""
import json
from pathlib import Path
import numpy as np
import pandas as pd
from sklearn.linear_model import LogisticRegression
from sklearn.preprocessing import StandardScaler
from sklearn.metrics import roc_auc_score

from train_baseline import FEATURES, by_game_split

HERE = Path(__file__).resolve().parent
FEATS = HERE.parent / "data" / "features_SOS_PremierDraft.parquet"
OUT = HERE.parent / "model" / "phi_logistic.json"


def main():
    df = pd.read_parquet(FEATS)
    X = df[FEATURES].values
    y = df["won"].values

    scaler = StandardScaler().fit(X)
    clf = LogisticRegression(max_iter=3000, C=1.0).fit(scaler.transform(X), y)

    # held-out sanity number (same by-game split as the baseline report)
    tr, te = by_game_split(df)
    p_te = clf.predict_proba(scaler.transform(df.loc[te, FEATURES].values))[:, 1]
    auc = roc_auc_score(df.loc[te, "won"].values, p_te)

    blob = {
        "features": FEATURES,
        "mean": scaler.mean_.tolist(),
        "scale": scaler.scale_.tolist(),
        "coef": clf.coef_[0].tolist(),
        "intercept": float(clf.intercept_[0]),
        "meta": {
            "set": "SOS", "event": "PremierDraft", "source": "17lands replay_data",
            "n_games": int(df["game_id"].nunique()), "n_rows": int(len(df)),
            "fit_on": "all sampled rows", "holdout_auc_by_game": round(float(auc), 4),
            "phi": "phi = 2*sigmoid(coef . ((x-mean)/scale) + intercept) - 1, in [-1,1]",
            "usage": "turn-boundary-only shaping potential; see README integration design",
        },
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(blob, indent=2))
    print(f"wrote {OUT}  (holdout AUC {auc:.4f}, {len(FEATURES)} features)")


if __name__ == "__main__":
    main()
