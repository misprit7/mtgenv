"""Flag-gated shaping-potential adapter for the winprob17l experiment.

Exposes a pure function `phi(features) -> float in [-1, 1]` backed by the frozen logistic
blob (model/phi_logistic.json). numpy-only at inference — NO sklearn/pandas/sqlite needed.

This is the ONLY intended integration point. It does not touch gym/training code; wiring it
in is a one-line flag seam proposed (not applied) in ../README.md. Until an in-gym A/B shows
it helps, the default Φ stays the current hand-tuned tanh heuristic.

The 23 features (order = blob["features"]) are all computable from the engine's PlayerView:
    turn, my_turn, on_play,
    my_life, opp_life, my_hand, opp_hand, my_lands, opp_lands,
    my_creatures, opp_creatures, my_noncreatures, opp_noncreatures,
    my_pow_sum, opp_pow_sum, my_tou_sum, opp_tou_sum, my_pow_max, opp_pow_max,
    my_cmc_sum, opp_cmc_sum, my_cmc_mean, opp_cmc_mean

`my_*`/`opp_*` are from the acting player's perspective. Creature aggregates are over that
side's creatures on the battlefield (P/T/CMC from the engine's own card characteristics).
Apply the shaping term F = γ·Φ(s') − Φ(s) ONLY at end-of-turn states (Φ is trained on and
defined for turn-boundary snapshots, not mid-combat).
"""
import json
from pathlib import Path
import numpy as np

_DEFAULT_BLOB = Path(__file__).resolve().parent.parent / "model" / "phi_logistic.json"


class WinProbPhi:
    def __init__(self, blob_path=_DEFAULT_BLOB):
        b = json.loads(Path(blob_path).read_text())
        self.features = list(b["features"])
        self.mean = np.asarray(b["mean"], dtype=np.float64)
        self.scale = np.asarray(b["scale"], dtype=np.float64)
        self.coef = np.asarray(b["coef"], dtype=np.float64)
        self.intercept = float(b["intercept"])
        self.meta = b.get("meta", {})

    def p_win(self, x):
        """P(win) for one row (len-23 vector) or a batch (N,23), matching `self.features` order."""
        x = np.asarray(x, dtype=np.float64)
        z = (x - self.mean) / self.scale
        logit = z @ self.coef + self.intercept
        return 1.0 / (1.0 + np.exp(-logit))

    def phi(self, x):
        """Shaping potential Φ = 2·P(win) − 1 ∈ [−1, 1]. Scalar for one row, (N,) for a batch."""
        return 2.0 * self.p_win(x) - 1.0

    def phi_from_dict(self, feats: dict) -> float:
        """Φ from a {feature_name: value} dict (missing keys default to 0.0)."""
        x = np.array([float(feats.get(f, 0.0)) for f in self.features], dtype=np.float64)
        return float(self.phi(x))


if __name__ == "__main__":
    # Smoke test: neutral opening state ≈ Φ near 0; a dominating board ≈ Φ strongly positive.
    p = WinProbPhi()
    neutral = {f: 0.0 for f in p.features}
    neutral.update(my_life=20, opp_life=20, my_hand=7, opp_hand=7, turn=1)
    ahead = dict(neutral)
    ahead.update(turn=8, my_life=20, opp_life=6, my_lands=6, opp_lands=4,
                 my_creatures=4, opp_creatures=1, my_hand=4, opp_hand=3,
                 my_pow_sum=10, opp_pow_sum=2, my_tou_sum=11, opp_tou_sum=3,
                 my_pow_max=4, opp_pow_max=2, my_cmc_sum=12, opp_cmc_sum=4,
                 my_cmc_mean=3, opp_cmc_mean=2)
    behind = dict(ahead)
    behind.update(my_life=6, opp_life=20, my_lands=4, opp_lands=6,
                  my_creatures=1, opp_creatures=4, my_pow_sum=2, opp_pow_sum=10,
                  my_tou_sum=3, opp_tou_sum=11)
    print(f"meta: {p.meta}")
    print(f"Φ(neutral opening) = {p.phi_from_dict(neutral):+.3f}")
    print(f"Φ(clearly ahead)   = {p.phi_from_dict(ahead):+.3f}")
    print(f"Φ(clearly behind)  = {p.phi_from_dict(behind):+.3f}")
