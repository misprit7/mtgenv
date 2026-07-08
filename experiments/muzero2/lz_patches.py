"""Local in-memory patches for LightZero v0.2.0 (site-packages stays pristine).

Patch 1 — random-collect exploration floor for Gumbel-MuZero AND Stochastic MuZero.
    LightZero's ``LightZeroRandomPolicy`` (used to seed the buffer with random-play games when
    ``random_collect_episode_num > 0``) only implements the muzero / efficientzero / sampled_efficientzero
    pipelines and raises ``NotImplementedError`` for ``gumbel_muzero`` / ``stochastic_muzero``. But both
    use the *same* MuZero model + MuZero MCTS + MuZero buffer format — the random-collect phase just runs
    MCTS for a (weak) visit-count target and plays a RANDOM legal action, which is pipeline-agnostic. So
    we alias both to the muzero path inside the random policy: build the MuZero MCTS/model and present
    ``type='muzero'`` to ``_forward_collect``'s branch checks. This keeps the exploration floor (a key
    lever) for Gumbel and Stochastic MuZero too.
      * Gumbel additionally reads ``improved_policy_probs`` (length action_space_size) and
        ``roots_completed_value`` per step; the muzero forward doesn't emit them, so for the seed data we
        scatter the MCTS visit distribution into a full-length policy target and use the searched value.
      * Stochastic MuZero reads neither, so no extra fields are synthesized for it.

Patch 2 — Stochastic MuZero ``timestep`` kwarg drift (v0.2.0).
    ``muzero_collector.py`` / ``muzero_evaluator.py`` call ``policy.forward(..., timestep=...)``. The
    MuZero/Gumbel ``_forward_collect``/``_forward_eval`` absorb it via ``**kwargs``, but the
    ``StochasticMuZeroPolicy`` equivalents were never given ``**kwargs`` (their signatures end at
    ``ready_env_id``), so collection/eval crash with an unexpected-keyword ``TypeError``. ``timestep`` is
    unused inside ``forward`` (only in ``_process_transition``, handled separately), so we drop it.

EfficientZero and plain MuZero need no patch (native random-collect + ``**kwargs`` forwards). UniZero
uses ``train_unizero`` (its own collector, no LightZeroRandomPolicy random-collect) so it is untouched.
"""
from __future__ import annotations

import functools

import numpy as np

_ALIAS_TO_MUZERO = {'gumbel_muzero', 'stochastic_muzero'}


def _apply_random_collect_alias():
    from lzero.policy import random_policy as rp
    from lzero.mcts import MuZeroMCTSCtree, MuZeroMCTSPtree
    from ding.policy.base_policy import Policy

    Orig = rp.LightZeroRandomPolicy
    if getattr(Orig, "_mtg_patched", False):
        return
    orig_init = Orig.__init__
    orig_fc = Orig._forward_collect
    orig_dm = Orig.default_model

    def __init__(self, cfg, model=None, enable_field=None, action_space=None):
        if getattr(cfg, "type", None) in _ALIAS_TO_MUZERO:
            self.MCTSCtree = MuZeroMCTSCtree
            self.MCTSPtree = MuZeroMCTSPtree
            self.action_space = action_space
            Policy.__init__(self, cfg, model, enable_field)   # bypass Orig.__init__'s type check
        else:
            orig_init(self, cfg, model, enable_field, action_space)

    def default_model(self):
        if getattr(self._cfg, "type", None) in _ALIAS_TO_MUZERO:
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
        if real in _ALIAS_TO_MUZERO:
            self._cfg.type = 'muzero'          # present as muzero for the branch selection
            try:
                output = orig_fc(self, data, action_mask, temperature, to_play, epsilon, ready_env_id)
            finally:
                self._cfg.type = real
            if real == 'gumbel_muzero':
                # The gumbel collector reads v['improved_policy_probs'] (length action_space_size) and
                # v['roots_completed_value'] (per-root scalar) every step; the muzero forward emits
                # neither. For random-collect seed data, scatter the MCTS visit distribution (over legal,
                # in-order) into a full-length policy target and use the searched root value.
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
    Orig._mtg_patched = True


def _apply_stochastic_timestep_strip():
    from lzero.policy.stochastic_muzero import StochasticMuZeroPolicy

    def _strip_timestep(fn):
        if getattr(fn, "_timestep_stripped", False):
            return fn

        @functools.wraps(fn)
        def wrapper(self, *args, **kwargs):
            kwargs.pop("timestep", None)
            return fn(self, *args, **kwargs)

        wrapper._timestep_stripped = True
        return wrapper

    StochasticMuZeroPolicy._forward_collect = _strip_timestep(StochasticMuZeroPolicy._forward_collect)
    StochasticMuZeroPolicy._forward_eval = _strip_timestep(StochasticMuZeroPolicy._forward_eval)


_apply_random_collect_alias()
_apply_stochastic_timestep_strip()
