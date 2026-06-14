"""Batched policy inference — the reusable evaluation primitive (GYM_PLAN §6, task #41).

The training bottleneck is **per-env opponent inference**: each opponent decision is a 1-sample
torch forward, and a 1-sample forward costs almost exactly as much as a 64-sample one (launch
overhead dominates — see ``bench_infer.py``). So evaluating K states in a *single* forward is ~Kx
cheaper. ``BatchedPolicy`` is that single-forward primitive.

It is deliberately decision-procedure-agnostic so it outlives the self-play opponent it's built for:

  * ``act`` — sample/argmax one action per state. Used now by the batched self-play pump (the
    opponent for K games at once).
  * ``evaluate`` — masked action **priors** + state **value** per state, one forward. This is the
    exact signature an AlphaZero-style MCTS (PUCT priors + leaf value) or a policy-guided minimax
    (move ordering + evaluation) wants — so tree search reuses this unchanged, just calling it on
    batches of *leaf* states instead of *opponent-turn* states.

No threads, no queues: batching happens because the *caller* hands us a batch. The thing that
*collects* batches (the lockstep pump for self-play, the leaf-gathering traversal for MCTS) is a
separate concern layered on top — see ``batched_selfplay.py``.
"""

from __future__ import annotations

import numpy as np
import torch


class BatchedPolicy:
    """Wraps one (frozen) SB3 ``MaskableActorCriticPolicy`` for batched, no-grad evaluation.

    Accepts a ``MaskablePPO`` model or a bare policy. All methods take *lists* of per-state
    observations (each a ``{name: np.ndarray}`` dict, as ``MtgEnv`` emits) and boolean action
    masks, stack them once, and run a single forward on the policy's device.
    """

    def __init__(self, model_or_policy):
        policy = getattr(model_or_policy, "policy", model_or_policy)
        self.policy = policy
        self.policy.set_training_mode(False)
        self.device = policy.device

    # ── batch assembly ────────────────────────────────────────────────────────────────────────
    @staticmethod
    def _stack_obs(obs_list):
        return {k: np.stack([o[k] for o in obs_list]) for k in obs_list[0]}

    # ── decision procedures ───────────────────────────────────────────────────────────────────
    def act(self, obs_list, mask_list, deterministic=False) -> np.ndarray:
        """One action per state (shape ``(K,)``), sampled (or argmax if ``deterministic``) from the
        legal-masked policy. Empty input → empty array."""
        if not obs_list:
            return np.empty((0,), dtype=np.int64)
        obs = self._stack_obs(obs_list)
        masks = np.stack(mask_list)
        actions, _ = self.policy.predict(obs, action_masks=masks, deterministic=deterministic)
        return np.asarray(actions, dtype=np.int64).reshape(-1)

    @torch.no_grad()
    def evaluate(self, obs_list, mask_list):
        """Masked action **priors** ``(K, A)`` and state **values** ``(K,)`` in one forward — the
        leaf-evaluation primitive for tree search. Priors are the policy's probabilities with
        illegal actions zeroed (they already are, under the mask); each legal row sums to ~1."""
        if not obs_list:
            a = self.policy.action_space.n
            return np.empty((0, a), dtype=np.float32), np.empty((0,), dtype=np.float32)
        obs = self._stack_obs(obs_list)
        masks = np.stack(mask_list)  # get_distribution applies the mask from a numpy array
        obs_t, _ = self.policy.obs_to_tensor(obs)
        dist = self.policy.get_distribution(obs_t, action_masks=masks)
        priors = dist.distribution.probs                      # (K, A), illegal already masked to 0
        values = self.policy.predict_values(obs_t).reshape(-1)  # (K,)
        return priors.cpu().numpy(), values.cpu().numpy()
