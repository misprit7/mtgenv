"""mtgenv_gym — the Gymnasium RL environment over the mtg-core engine (GYM_PLAN L4, milestone 0).

The Rust extension ``mtg_py`` (crates/mtg-py) does the engine work; this package is the thin Python
layer: a Gym ``MtgEnv`` and a low-level random self-play driver.
"""

from .env import MtgEnv, random_action
from .selfplay import GameStats, play_random_game

__all__ = ["MtgEnv", "random_action", "GameStats", "play_random_game"]

# League + batched inference are imported lazily (they pull torch); expose the names for discovery.
__all__ += ["OpponentPool", "ModelOpponent", "PoolCheckpoint", "BatchedPolicy",
            "BatchedSelfPlayVecEnv", "FleetSelfPlayVecEnv"]


def __getattr__(name):  # PEP 562 — lazy so `import mtgenv_gym` stays torch-free
    if name in ("OpponentPool", "ModelOpponent", "PoolCheckpoint"):
        from . import league

        return getattr(league, name)
    if name == "BatchedPolicy":
        from .inference import BatchedPolicy

        return BatchedPolicy
    if name == "BatchedSelfPlayVecEnv":
        from .batched_selfplay import BatchedSelfPlayVecEnv

        return BatchedSelfPlayVecEnv
    if name == "FleetSelfPlayVecEnv":
        from .fleet_selfplay import FleetSelfPlayVecEnv

        return FleetSelfPlayVecEnv
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")

# Optional Gymnasium registration (so `gym.make("Mtg-v0")` works); harmless if gym is absent.
try:  # pragma: no cover
    from gymnasium.envs.registration import register

    register(id="Mtg-v0", entry_point="mtgenv_gym.env:MtgEnv")
except Exception:  # pragma: no cover
    pass
