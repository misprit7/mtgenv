"""``evaluate_checkpoint`` — the plain-function eval hook any training loop (or backfill) can call.

This is the algorithm-agnostic generalization of the MuZero ``muzero_observability`` per-checkpoint
harness: given a ``Policy`` and a step, it plays the standard eval battery (greedy AND sampled), logs
the canonical TB tags into the run dir, writes the JSON artifact, and records one replay — with no SB3
callback machinery. A custom trainer calls this from its own loop; the offline CLI calls it for
backfills. Unlike the in-training :class:`~mtgenv_gym.evalkit.sb3.EvalkitCallback` (where behaviour
stats come from the training rollout stream), here ``stats/*`` + ``game/*`` are computed from the
vs-random eval games (the canonical offline behaviour source, as the MuZero harness did).
"""

from __future__ import annotations

import os

from .arena import Arena
from .policy import RandomPolicy
from .replay import REPLAY_DIR, record_game
from .tb_logging import WriterRecorder, log_eval, write_json


def evaluate_checkpoint(policy, step: int, run_dir: str, *, deck: str, opponents=None,
                        games: int = 100, batch_size: int = 64, seed_random: int = 5_000_000,
                        modes=("greedy", "sample"), record_replay: bool = True, algo: str = "POLICY",
                        run_name: "str | None" = None, writer=None, analyzers: bool = True,
                        arena: "Arena | None" = None) -> "dict[str, dict]":
    """Evaluate ``policy`` at ``step`` and log/write everything into ``run_dir``.

    ``opponents``: ordered ``{win_tag: Policy}`` (default ``{"selfplay/winrate_vs_random":
    RandomPolicy()}``). The ``selfplay/winrate_vs_random`` opponent additionally carries ``stats/*`` +
    ``game/*`` + deck analyzers; others log win-rate (+ ``_sampled``) only. Returns
    ``{win_tag: {mode: EvalResult}}``. ``writer`` may be a shared torch ``SummaryWriter`` (else one is
    opened at ``run_dir`` and closed)."""
    arena = arena or Arena(deck, batch_size=batch_size)
    if opponents is None:
        opponents = {"selfplay/winrate_vs_random": RandomPolicy(seed=seed_random)}

    own_writer = writer is None
    if own_writer:
        from torch.utils.tensorboard import SummaryWriter

        writer = SummaryWriter(run_dir)
    recorder = WriterRecorder(writer)

    results: "dict[str, dict]" = {}
    labelled = {}
    for i, (win_tag, opp) in enumerate(opponents.items()):
        res = arena.evaluate(policy, opp, n_games=games, seed=seed_random + i * 1_000_000,
                             opponent_label=win_tag, modes=modes)
        behaviour = (win_tag == "selfplay/winrate_vs_random")  # canonical offline behaviour source
        log_eval(recorder, res, win_tag=win_tag, step=step, with_stats=behaviour,
                 with_game=behaviour, with_analyzers=behaviour and analyzers)
        results[win_tag] = res
        for m, r in res.items():
            labelled[f"{win_tag.rsplit('/', 1)[-1]}_{m}"] = r

    json_path = write_json(run_dir, step, labelled)
    replay_path = None
    if record_replay:
        replay_path = record_game(policy, deck, step, out_dir=REPLAY_DIR, run_name=run_name, algo=algo)

    writer.flush()
    if own_writer:
        writer.close()
    if os.environ.get("EVALKIT_VERBOSE"):
        g = results["selfplay/winrate_vs_random"]["greedy"]
        print(f"  step={step}: vs_random greedy={g.win_rate:.3f}  json={os.path.basename(json_path)}"
              f"  replay={os.path.basename(replay_path) if replay_path else '—'}")
    return results
