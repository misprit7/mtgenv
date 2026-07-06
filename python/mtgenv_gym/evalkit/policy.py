"""The ``Policy`` protocol — THE integration point of evalkit.

Every algorithm (PPO today; MCTS/AZ-family, DMC, … tomorrow) plugs into the eval/metrics/logging
stack by exposing **one** thing: a batched ``act``. Everything else in evalkit (``Arena``, ``Ladder``,
the loggers, the CLI) is written against this protocol, so integrating a new algorithm is writing a
thin adapter — see ``README.md`` ("add an algorithm in ~20 lines").

Batched-first by design: ``act`` answers a whole *round* of games at once (one entry per game sitting
at a decision this round). A plain NN policy stacks the batch into a single forward (reusing
``BatchedPolicy``); a search policy runs its per-decision budget over the batch of roots. This is the
same "advance every game to a decision, evaluate the batch" shape the self-play pump and batched-MCTS
leaf evaluation use, so nothing here is PPO-specific.

    def act(self, obs_batch, mask_batch, *, mode="greedy", env_indices=None) -> np.ndarray

* ``obs_batch``   — list of ``K`` observation dicts (each ``{name: np.ndarray}`` exactly as
  ``MtgEnv`` emits — the card-id one-hots included).
* ``mask_batch``  — list of ``K`` boolean legal-action masks (shape ``(action_dim,)`` each).
* ``mode``        — ``"greedy"`` (argmax) or ``"sample"`` (stochastic). The evaluated policy is run
  in both (the MuZero lesson: greedy is the headline, sampled is the honest learning-signal curve).
* ``env_indices`` — the stable per-game slot id of each row (optional). Stateless policies **ignore
  it** (absorb via ``**kwargs``); stateful ones (a search tree per game, or a per-game RNG stream)
  use it to route state. Arena always passes it.

Returns an ``int`` action per row, shape ``(K,)``.

Optional ``reset(env_indices, *, rng)`` lets a stateful/search policy drop per-game state and (re)seed
when Arena (re)starts those slots; the ``BasePolicy`` mixin no-ops it so most adapters need only
``act``.
"""

from __future__ import annotations

from typing import Protocol, Sequence, runtime_checkable

import numpy as np


@runtime_checkable
class Policy(Protocol):
    """The one interface an algorithm implements to be evaluable. See the module docstring."""

    def act(self, obs_batch: "list[dict[str, np.ndarray]]", mask_batch: "list[np.ndarray]", *,
            mode: str = "greedy", env_indices: "Sequence[int] | None" = None) -> np.ndarray: ...


class BasePolicy:
    """Convenience mixin: a no-op ``reset`` so stateless adapters need only implement ``act``."""

    def reset(self, env_indices: "Sequence[int]", *, rng: "np.random.Generator | None" = None,
              game_seeds: "Sequence[int] | None" = None) -> None:
        pass


class RandomPolicy(BasePolicy):
    """Uniform-random-legal policy — the win-rate-vs-random baseline (and the empty-pool opponent).

    Torch-free (so it works in any venv). Per-game RNG streams keyed by slot: ``reset`` seeds each
    slot from its game seed the **same way** ``MtgEnv``'s internal random opponent does
    (``default_rng(game_seed ^ 0x5DEECE66D)``), so an Arena game driven with a ``RandomPolicy``
    opponent is bit-identical to the legacy ``MtgEnv(opponent="random")`` path — the migration keeps
    win-rate-vs-random numbers, not just the tag. ``mode`` is irrelevant (uniform either way)."""

    _XOR = 0x5DEECE66D
    _U64 = (1 << 64) - 1

    def __init__(self, seed: int = 0):
        self._base = int(seed)
        self._rngs: "dict[int, np.random.Generator]" = {}

    def reset(self, env_indices, *, rng=None, game_seeds=None):
        for j, i in enumerate(env_indices):
            s = int(game_seeds[j]) if game_seeds is not None else (self._base + int(i))
            self._rngs[int(i)] = np.random.default_rng((s ^ self._XOR) & self._U64)

    def _rng_for(self, i: int) -> "np.random.Generator":
        r = self._rngs.get(int(i))
        if r is None:  # never reset for this slot → fall back to a base-seeded stream
            r = np.random.default_rng((self._base + int(i)) ^ self._XOR & self._U64)
            self._rngs[int(i)] = r
        return r

    def act(self, obs_batch, mask_batch, *, mode="greedy", env_indices=None):
        n = len(mask_batch)
        idx = list(env_indices) if env_indices is not None else list(range(n))
        out = np.empty(n, dtype=np.int64)
        for k in range(n):
            legal = np.flatnonzero(np.asarray(mask_batch[k], dtype=bool))
            assert legal.size, "empty action mask for a random decision"
            out[k] = int(self._rng_for(idx[k]).choice(legal))
        return out


class SearchPolicy(BasePolicy):
    """Documented STUB for a search-based algorithm (MCTS / AlphaZero-family / policy-guided minimax).

    A search policy differs from a reactive NN policy in two ways evalkit already accommodates:

    1. **Per-decision compute budget.** ``act`` for a search policy runs ``num_simulations`` (or a
       time budget) of tree expansion *per root*, over the whole batch of roots handed in. Keep the
       budget an attribute so the offline CLI / ``evaluate_checkpoint`` can sweep it. Report the
       budget you evaluated at — a win-rate is only comparable at a fixed budget.

    2. **Per-game state across calls.** A search tree (or reused subtree) is kept per game slot. Use
       ``env_indices`` to route each ``act`` row to its slot's tree, and ``reset(env_indices)`` to
       drop trees for slots Arena is (re)starting.

    The leaf-evaluation primitive is already built: ``mtgenv_gym.inference.BatchedPolicy.evaluate``
    returns masked PUCT **priors** ``(K, A)`` and state **values** ``(K,)`` in one forward — call it
    on a batch of *leaf* states exactly as the self-play pump calls ``act`` on a batch of
    *decision* states. A concrete implementation fills in ``act`` (and typically ``act_greedy`` =
    argmax visit counts, ``act_sample`` = visits^(1/temp)); this base intentionally raises.

    Note (fair-greedy tie-break, from the MuZero harness): break argmax-visit ties by the network
    prior, NOT by lowest action index — a weak net otherwise reads ~0 by always picking PASS."""

    def __init__(self, num_simulations: int = 50):
        self.num_simulations = int(num_simulations)

    def act(self, obs_batch, mask_batch, *, mode="greedy", env_indices=None):
        raise NotImplementedError(
            "SearchPolicy is a documented stub — subclass and implement act() over a batch of roots. "
            "See the class docstring: budget = num_simulations per root; leaf eval via "
            "mtgenv_gym.inference.BatchedPolicy.evaluate; keep per-game trees keyed by env_indices."
        )
