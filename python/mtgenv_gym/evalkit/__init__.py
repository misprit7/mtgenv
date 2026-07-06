"""evalkit — a stable, algorithm-agnostic evaluation / metrics / logging stack for the mtgenv RL gym.

Many algorithms (PPO today; MCTS/AlphaZero-family, DMC, … tomorrow) share ONE eval + metrics +
logging pipeline. Integrating a new algorithm = writing a thin ``Policy`` adapter (see ``README.md``,
"add an algorithm in ~20 lines"). Everything downstream — the ``Arena``, the ``Ladder``, the canonical
TensorBoard tags, the JSON artifacts, the deck analyzers, the replays — is written against the
``Policy`` protocol and reused unchanged.

Public API (one canonical import path — ``from mtgenv_gym.evalkit import X``):

    Policy, BasePolicy, RandomPolicy, SearchPolicy      # the integration point + baselines/stub
    Arena, EvalResult                                   # play N seeded games → structured result
    Ladder                                              # %-trained self-relative progress curve
    wilson_ci                                           # binomial CI used for win-rate bars
    Analyzer, get_analyzer, register_analyzer           # pluggable per-deck judgment analyzers
    log_eval, write_json, SB3Recorder, WriterRecorder   # canonical TB schema + JSON artifacts
    record_game                                         # policy-driven replay export
    evaluate_checkpoint                                 # plain-function hook for any custom loop
    SB3Policy, EvalkitCallback                          # SB3/MaskablePPO adapter + drop-in callback
    register_adapter, POLICY_ADAPTERS                   # CLI policy-loader registry

The core (everything except ``SB3Policy``/``EvalkitCallback``) is torch/sb3-free so it imports in an
isolated eval venv (e.g. LightZero's) that has ``mtg_py`` but not stable-baselines3.
"""

from __future__ import annotations

from .analyzers import Analyzer, get_analyzer, register_analyzer
from .arena import Arena, EvalResult
from .hooks import evaluate_checkpoint
from .ladder import Ladder
from .metrics import wilson_ci
from .policy import BasePolicy, Policy, RandomPolicy, SearchPolicy
from .replay import record_game
from .tb_logging import SB3Recorder, WriterRecorder, log_eval, write_json

__all__ = [
    "Policy", "BasePolicy", "RandomPolicy", "SearchPolicy",
    "Arena", "EvalResult", "Ladder", "wilson_ci",
    "Analyzer", "get_analyzer", "register_analyzer",
    "log_eval", "write_json", "SB3Recorder", "WriterRecorder", "record_game",
    "evaluate_checkpoint",
    # lazily exposed (pull sb3): SB3Policy, EvalkitCallback, register_adapter, POLICY_ADAPTERS
    "SB3Policy", "EvalkitCallback", "register_adapter", "POLICY_ADAPTERS",
]


def __getattr__(name):  # PEP 562 — keep `import mtgenv_gym.evalkit` sb3/torch-free
    if name in ("SB3Policy", "EvalkitCallback"):
        from . import sb3

        return getattr(sb3, name)
    if name in ("register_adapter", "POLICY_ADAPTERS"):
        from . import __main__ as cli

        return getattr(cli, name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
