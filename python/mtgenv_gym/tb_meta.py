"""Per-run TensorBoard metadata, shared by the training entrypoints.

Three things every run should carry so a run is self-describing in TensorBoard:

1. **Run notes** (``run/notes``, TEXT tab) — a freeform ``--notes`` description of what the run is
   testing plus an auto-generated config block (deck, budget, hyperparams, extractor, git SHA, date).
   SB3's logger has no text channel, so we reach the underlying ``SummaryWriter`` (via
   ``TensorBoardOutputFormat``) and call ``add_text`` directly.

2. **A Custom Scalars layout** (``add_custom_scalars``) — the CUSTOM SCALARS tab groups the handful of
   metrics that matter (Outcome / Behavior / Training) onto one screen. NOTE: true "pin cards" live in
   the browser's localStorage and can't be set server-side, so the Custom Scalars tab is the
   programmatic equivalent of a default dashboard.

3. **Game length** — SB3's ``rollout/ep_len_mean`` is only populated by a ``Monitor`` wrapper, which
   the batched self-play vec env bypasses; ``GameLengthCallback`` logs ``game/turns_mean`` and the
   end-reason mix from each episode's ``summary()`` instead.

Import-light (only SB3 + stdlib) so it never pulls torch/mtg_py at module load.
"""

from __future__ import annotations

import os
import re
import subprocess
from datetime import datetime, timezone

from stable_baselines3.common.callbacks import BaseCallback

_REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))

# ── Versioned run names ─────────────────────────────────────────────────────────────────────────
# Runs are named ``<major>.<minor>-<slug>`` (e.g. ``2.7-bears-live-60k``) so TensorBoard runs AND the
# lobby's AiTraining replay arcs sort in run order (not alphabetical-by-description) and correlate
# 1:1. The MINOR auto-increments — scan the TB root for the highest ``<major>.<minor>`` and +1 — so no
# counter file is needed for it. The MAJOR is bumped manually (``--run-major`` / a sticky
# ``<tb_root>/.run_major`` file) for a new reward/arch/deck era. ``versioned_run_name`` is the single
# seam both selfplay_train and export_replays route through; ``--run-name`` overrides it verbatim.
_VER_RE = re.compile(r"^(\d+)\.(\d+)-")


def _existing_versions(tb_root: str) -> list[tuple[int, int]]:
    try:
        names = os.listdir(tb_root)
    except OSError:
        return []
    return [(int(m.group(1)), int(m.group(2))) for d in names if (m := _VER_RE.match(d))]


def _resolve_major(tb_root: str, major: int | None) -> int:
    if major is not None:  # explicit --run-major: use it and make it sticky for later runs
        try:
            os.makedirs(tb_root, exist_ok=True)
            with open(os.path.join(tb_root, ".run_major"), "w") as f:
                f.write(str(int(major)))
        except OSError:
            pass
        return int(major)
    try:
        with open(os.path.join(tb_root, ".run_major")) as f:
            return int(f.read().strip())
    except (OSError, ValueError):
        pass
    return max((mj for mj, _ in _existing_versions(tb_root)), default=2)  # infer, default to the 2.x era


def versioned_run_name(tb_root: str, slug: str, major: int | None = None, override: str | None = None) -> str:
    """``<major>.<minor>-<slug>`` with an auto-incrementing minor (see module comment). ``override``
    (a ``--run-name``) short-circuits to that exact name."""
    if override:
        return override
    m = _resolve_major(tb_root, major)
    minor = max((mn for mj, mn in _existing_versions(tb_root) if mj == m), default=-1) + 1
    return f"{m}.{minor}-{slug}"


def git_sha() -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"], cwd=_REPO_ROOT, text=True,
            stderr=subprocess.DEVNULL,
        ).strip()
    except Exception:
        return "unknown"


# Curated dashboard. `add_custom_scalars` maps {category: {chart: [type, [tags...]]}}; tags that a run
# doesn't log simply render empty (e.g. block_rate on a deck with no blocking, ladder before wiring).
CUSTOM_SCALARS_LAYOUT = {
    "Outcome": {
        "win-rate": ["Multiline", ["selfplay/winrate_vs_random", "selfplay/winrate_vs_initial"]],
        "%-trained ladder": ["Multiline", [
            "ladder/winrate_vs_10pct", "ladder/winrate_vs_25pct",
            "ladder/winrate_vs_50pct", "ladder/winrate_vs_75pct",
        ]],
    },
    "Behavior": {
        "action rates": ["Multiline", [
            "stats/productive_rate", "stats/attack_rate", "stats/block_rate",
            "stats/cast_rate", "stats/playland_rate",
        ]],
    },
    "Training": {
        "shaping coef": ["Multiline", ["train/shaping_coef"]],
        "game length (turns)": ["Multiline", ["game/turns_mean"]],
    },
}


def build_notes(config: dict, notes: str | None) -> str:
    """Markdown for the TEXT tab: freeform notes (if any) + an auto config table."""
    now = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    full = dict(config)
    full.setdefault("git_sha", git_sha())
    full.setdefault("date", now)
    lines = ["# Run notes", ""]
    if notes:
        lines += [notes.strip(), ""]
    lines += ["## Config", "", "| key | value |", "| --- | --- |"]
    for k, v in full.items():
        lines.append(f"| {k} | {v} |")
    return "\n".join(lines)


def _writer(model):
    """The torch ``SummaryWriter`` behind SB3's logger, or None if TB logging is off."""
    from stable_baselines3.common.logger import TensorBoardOutputFormat

    for fmt in getattr(getattr(model, "logger", None), "output_formats", []) or []:
        if isinstance(fmt, TensorBoardOutputFormat):
            return fmt.writer
    return None


def write_run_metadata(model, config: dict, notes: str | None = None, layout=CUSTOM_SCALARS_LAYOUT) -> None:
    """Write the run-notes text (+ a Custom Scalars layout unless ``layout`` is None) once, at training
    start. No-op without a TensorBoard writer. Augments ``config`` with the model's extractor. Pass
    ``layout=None`` for trainers that log a different metric set (e.g. the vs-random ``train.py``)."""
    w = _writer(model)
    if w is None:
        return
    cfg = dict(config)
    try:
        fe = model.policy.features_extractor
        cfg.setdefault("extractor", type(fe).__name__)
        cfg.setdefault("features_dim", getattr(fe, "features_dim", "?"))
    except Exception:
        pass
    w.add_text("run/notes", build_notes(cfg, notes), 0)
    if layout is not None:
        w.add_custom_scalars(layout)
    w.flush()


class RunMetadataCallback(BaseCallback):
    """At training start, write the run-notes text (+ Custom Scalars dashboard unless ``layout`` None)."""

    def __init__(self, config: dict, notes: str | None = None, layout=CUSTOM_SCALARS_LAYOUT):
        super().__init__()
        self.config = config
        self.notes = notes
        self.layout = layout

    def _on_training_start(self) -> None:
        write_run_metadata(self.model, self.config, self.notes, layout=self.layout)

    def _on_step(self) -> bool:
        return True


class GameLengthCallback(BaseCallback):
    """Log ``game/turns_mean`` (avg game length in turns) + the end-reason mix each rollout, read from
    each finished episode's ``summary()`` (surfaced as ``info['episode_summary']`` by the batched
    self-play pump). Fills the game-length gap left by bypassing SB3's Monitor wrapper."""

    def __init__(self):
        super().__init__()
        self._turns: list[int] = []
        self._reasons: dict[str, int] = {}

    def _on_step(self) -> bool:
        for info in self.locals.get("infos", []) or []:
            s = info.get("episode_summary")
            if s:
                self._turns.append(int(s.get("turns", 0)))
                r = str(s.get("reason", "?"))
                self._reasons[r] = self._reasons.get(r, 0) + 1
        return True

    def _on_rollout_end(self) -> None:
        if self._turns:
            self.logger.record("game/turns_mean", sum(self._turns) / len(self._turns))
            total = sum(self._reasons.values()) or 1
            for r, c in self._reasons.items():
                self.logger.record(f"game/end_{r}", c / total)
        self._turns.clear()
        self._reasons.clear()
