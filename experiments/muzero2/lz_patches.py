"""Local in-memory patches for LightZero v0.2.0 (site-packages stays pristine).

Patch — Gumbel-MuZero random-collect exploration floor.
    LightZero's ``LightZeroRandomPolicy`` (used to seed the buffer with random-play games when
    ``random_collect_episode_num > 0``) only implements the muzero / efficientzero / sampled_efficientzero
    pipelines and raises ``NotImplementedError: need to implement pipeline: gumbel_muzero``. But Gumbel
    MuZero uses the *same* MuZero model + MuZero MCTS + MuZero buffer format — the random-collect phase
    just runs MCTS for a (weak) visit-count target and plays a RANDOM legal action, which is
    pipeline-agnostic. So we alias gumbel_muzero to the muzero path inside the random policy: build the
    MuZero MCTS/model and present ``type='muzero'`` to ``_forward_collect``'s branch checks. This lets us
    keep the exploration floor (a key never-tried lever) for Gumbel too.

Note: neither ``muzero`` nor ``gumbel_muzero`` needs the old stochastic-muzero ``timestep`` kwarg patch
(both ``_forward_collect``/``_forward_eval`` already take ``**kwargs``), so that patch is not ported.
"""
from __future__ import annotations

import numpy as np


def _apply_gumbel_random_collect():
    from lzero.policy import random_policy as rp
    from lzero.mcts import MuZeroMCTSCtree, MuZeroMCTSPtree
    from ding.policy.base_policy import Policy

    Orig = rp.LightZeroRandomPolicy
    if getattr(Orig, "_gumbel_patched", False):
        return
    orig_init = Orig.__init__
    orig_fc = Orig._forward_collect
    orig_dm = Orig.default_model

    def __init__(self, cfg, model=None, enable_field=None, action_space=None):
        if getattr(cfg, "type", None) == 'gumbel_muzero':
            self.MCTSCtree = MuZeroMCTSCtree
            self.MCTSPtree = MuZeroMCTSPtree
            self.action_space = action_space
            Policy.__init__(self, cfg, model, enable_field)   # bypass Orig.__init__'s type check
        else:
            orig_init(self, cfg, model, enable_field, action_space)

    def default_model(self):
        if getattr(self._cfg, "type", None) == 'gumbel_muzero':
            if self._cfg.model.model_type == 'mlp':
                return 'MuZeroModelMLP', ['lzero.model.muzero_model_mlp']
            return 'MuZeroModel', ['lzero.model.muzero_model']
        return orig_dm(self)

    def _forward_collect(self, data, action_mask=None, temperature=1, to_play=[-1], epsilon=0.25,
                         ready_env_id=None, **kwargs):
        # The collector passes timestep=...; the original LightZeroRandomPolicy._forward_collect has no
        # **kwargs, so absorb+drop it here (it is unused by the random-collect MCTS).
        kwargs.pop("timestep", None)
        real = getattr(self._cfg, "type", None)
        if real == 'gumbel_muzero':
            self._cfg.type = 'muzero'          # present as muzero for the branch selection
            try:
                output = orig_fc(self, data, action_mask, temperature, to_play, epsilon, ready_env_id)
            finally:
                self._cfg.type = real
            # The gumbel collector reads v['improved_policy_probs'] (length action_space_size) for every
            # step; the muzero forward doesn't emit it. For random-collect seed data, scatter the MCTS
            # visit distribution (over legal, in-order) into a full-length policy target.
            # Gumbel collector also reads v['roots_completed_value'] (a per-root scalar); use the
            # searched root value as the seed-data placeholder.
            A = int(self._cfg.model.action_space_size)
            for j, v in enumerate(output.values()):
                legal = [i for i, x in enumerate(action_mask[j]) if x == 1]
                dist = np.asarray(v['visit_count_distributions'], dtype=np.float32)
                p = np.zeros(A, dtype=np.float32)
                p[legal] = (dist / dist.sum()) if dist.sum() > 0 else (1.0 / max(len(legal), 1))
                v['improved_policy_probs'] = p
                v['roots_completed_value'] = v.get('searched_value', 0.0)
            return output
        return orig_fc(self, data, action_mask, temperature, to_play, epsilon, ready_env_id)

    Orig.__init__ = __init__
    Orig.default_model = default_model
    Orig._forward_collect = _forward_collect
    Orig._gumbel_patched = True


_apply_gumbel_random_collect()
