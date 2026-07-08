"""Top-N-Elo eval opponents (evalkit.sb3.load_elo_opponents): reads data/elo/<env>/ratings.json
read-only and builds the strongest agents as eval opponents. Fail-soft when there's no rating pool."""

from mtgenv_gym.evalkit.sb3 import load_elo_opponents


def test_no_ratings_file_is_fail_soft():
    assert load_elo_opponents("nonexistent_env_xyz", top=3) == []


def test_swine_top3_loads_when_present():
    import os

    from mtgenv_gym.evalkit.ratings import elo_root

    if not (elo_root() / "swine" / "ratings.json").is_file():
        return  # no rating pool on this box → nothing to assert (the metric just skips)
    elo = load_elo_opponents("swine", top=3, device="cpu", exclude=("self-run",))
    assert 1 <= len(elo) <= 3
    names = [n for n, _ in elo]
    assert "random" not in names, "the 1000 anchor must be excluded"
    assert len(names) == len(set(names)), "no duplicates"
