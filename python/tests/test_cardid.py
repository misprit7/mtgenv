"""Deck-determined card-identity one-hot in the observation (GYM_PLAN §3).

The Rust encoder stays card-agnostic (emits a per-row `grp_id`); the env adds an explicit one-hot
whose categories are fixed from the matchup's unique cards (`PyGame.card_vocab()`) + a token reserve
+ an "unknown" slot. These pin: the dimension, that every card-bearing row is exactly one-hot at its
grp_id's index, empty rows are all-zero, and the policy still trains over the augmented obs.
"""

import numpy as np

from mtgenv_gym import MtgEnv


def test_cardid_dim_and_space():
    env = MtgEnv(deck="demo")
    vocab = len(env._cardid_index)
    assert env._cardid_dim == vocab + 8 + 1, "vocab + 8 token-reserve + 1 unknown"
    for t in ("bf", "hand", "stack"):
        sp = env.observation_space.spaces[f"{t}_cardid"]
        assert sp.shape[-1] == env._cardid_dim
        # rows match the matching *_ids table width
        assert sp.shape[0] == env.observation_space.spaces[f"{t}_ids"].shape[-1]


def test_cardid_is_onehot_at_the_right_index():
    env = MtgEnv(deck="demo")
    o, info = env.reset(seed=2)
    rng = np.random.default_rng(0)
    saw_present = False
    for _ in range(400):
        for t in ("bf", "hand", "stack"):
            ids, oh = o[f"{t}_ids"], o[f"{t}_cardid"]
            present = ids != 0
            sums = oh.sum(axis=1)
            assert np.all(sums[present] == 1.0), "present row is exactly one-hot"
            assert np.all(sums[~present] == 0.0), "empty row is all-zero"
            for r in np.flatnonzero(present):
                idx = env._cardid_index.get(int(ids[r]), env._cardid_unknown)
                assert oh[r, idx] == 1.0, "one-hot sits at the grp_id's vocab index"
                saw_present = True
        a = int(rng.choice(np.flatnonzero(info["action_mask"])))
        o, _r, term, trunc, info = env.step(a)
        if term or trunc:
            break
    assert saw_present, "expected at least one card-bearing row during a game"


def test_unknown_grp_id_falls_in_reserved_slot():
    env = MtgEnv(deck="demo")
    # a grp_id not in the vocab maps to the reserved 'unknown' slot, never out of range
    assert env._cardid_index.get(999_999, env._cardid_unknown) == env._cardid_unknown
    assert env._cardid_unknown == env._cardid_dim - 1


def test_selesnya_vocab_larger_than_demo():
    # the one-hot scales with the matchup's unique-card count (sanity that it's deck-determined)
    assert len(MtgEnv(deck="selesnya")._cardid_index) > len(MtgEnv(deck="demo")._cardid_index)
