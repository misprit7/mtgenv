"""rate_agent obs-schema adapter — rating checkpoints trained on an older (narrower) obs schema.

The gym obs only ever grows by appending columns, so an old model is fed the current obs truncated to
its own per-key shape. Pure-numpy coverage of the shape coercion + the adapter's key handling (no
engine / torch).
"""

import os
import sys

import numpy as np
from gymnasium import spaces

sys.path.insert(0, os.path.dirname(os.path.dirname(__file__)))  # python/ — for the top-level runner
import rate_agent  # noqa: E402


def test_fit_shape_truncates_and_pads():
    a = np.arange(6, dtype=np.float32).reshape(2, 3)
    assert rate_agent._fit_shape(a, (2, 3)) is a                       # identity when equal
    assert rate_agent._fit_shape(a, (2, 2)).tolist() == [[0, 1], [3, 4]]  # truncate the wide axis
    p = rate_agent._fit_shape(a, (3, 3))                                # pad the short axis with zeros
    assert p.shape == (3, 3) and p[2].tolist() == [0, 0, 0]


class _Recorder:
    def act(self, obs_batch, mask_batch, *, mode="greedy", env_indices=None):
        self.seen = {k: np.asarray(v).shape for k, v in obs_batch[0].items()}
        return np.zeros(len(obs_batch), dtype=np.int64)


def test_schema_adapter_truncates_and_drops_extra_keys():
    # model expects a 44-wide bf_feat; current engine emits 48 + an extra key the model never saw.
    space = spaces.Dict({
        "bf_feat": spaces.Box(-9, 9, (32, 44), np.float32),
        "globals": spaces.Box(-9, 9, (69,), np.float32),
    })
    rec = _Recorder()
    adapter = rate_agent._SchemaAdapter(rec, space)
    obs = {
        "bf_feat": np.arange(32 * 48, dtype=np.float32).reshape(32, 48),
        "globals": np.zeros(69, np.float32),
        "relation_extra": np.zeros(3, np.float32),   # a key the model doesn't expect
    }
    adapter.act([obs], [np.ones(98, bool)])
    assert rec.seen["bf_feat"] == (32, 44)            # truncated to the model's width
    assert rec.seen["globals"] == (69,)
    assert "relation_extra" not in rec.seen           # dropped
