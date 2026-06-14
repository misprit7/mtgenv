"""tracked_stats (#68): accumulator ratios, registry extensibility, and that the env emits a
per-decision record into info['decision_stats']."""
import numpy as np

from mtgenv_gym import MtgEnv
from mtgenv_gym.tracked_stats import REGISTRY, StatAccumulator, StatDef


def test_accumulator_ratio_and_nan_until_observed():
    acc = StatAccumulator()
    acc.update({"cast_legal": 1.0, "cast_taken": 1.0})  # an opportunity, taken
    acc.update({"cast_legal": 1.0, "cast_taken": 0.0})  # an opportunity, passed
    acc.update({"attack_eligible": 3.0, "attack_declared": 2.0})
    log = acc.as_log_dict()
    assert log["stats/cast_rate"] == 0.5
    assert log["stats/cast_rate_num"] == 1.0 and log["stats/cast_rate_den"] == 2.0
    assert abs(log["stats/attack_rate"] - 2 / 3) < 1e-6
    # block_rate never observed a denominator → NaN (a TB gap, not a divide-by-zero)
    assert log["stats/block_rate"] != log["stats/block_rate"]


def test_registry_is_one_entry_extensible():
    custom = REGISTRY + [
        StatDef("activate_rate", lambda r: (r.get("activate_taken", 0.0), r.get("activate_legal", 0.0)))
    ]
    acc = StatAccumulator(custom)
    acc.update({"activate_legal": 1.0, "activate_taken": 1.0})
    assert acc.as_log_dict()["stats/activate_rate"] == 1.0


def test_reset_clears():
    acc = StatAccumulator()
    acc.update({"cast_legal": 1.0, "cast_taken": 1.0})
    acc.reset()
    assert acc.as_log_dict()["stats/cast_rate_den"] == 0.0


def test_env_emits_decision_stats_records():
    env = MtgEnv(deck="selesnya", opponent="random")
    rng = np.random.default_rng(0)
    saw = 0
    for ep in range(12):
        obs, info = env.reset(seed=ep)
        done = False
        while not done:
            legal = np.flatnonzero(info["action_mask"])
            obs, r, term, trunc, info = env.step(int(rng.choice(legal)))
            rec = info.get("decision_stats")
            if rec:
                saw += 1
                assert all(isinstance(v, float) for v in rec.values())
            done = term or trunc
    assert saw > 0, "env never emitted a decision_stats record over 12 games"
