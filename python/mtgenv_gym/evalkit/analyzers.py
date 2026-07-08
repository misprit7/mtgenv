"""Pluggable per-deck judgment analyzers.

Some decks have a *specific* strategic question that a generic win-rate/attack-rate can't see. The
swine deck's is the user's combat-judgment concern: bears (2/2) vs a trampling 3/3 Swine. An analyzer
watches the evaluated policy's decisions during Arena play and emits deck-specific scalars, logged
automatically **when the deck matches** — no caller wiring. Adding one is a factory in ``ANALYZERS``
(or ``register_analyzer``).

An analyzer sees, per finalized decision of the evaluated policy, the ``obs`` it acted on (pre-apply)
and the ``decision_stats`` record that decision produced. It reads the board straight from ``obs``
(``bf_feat`` power/tapped/attacking/blocked-by/is_pending_combat columns), so it can classify combat
by creature identity (a 3-power creature is the Swine; a 2-power one is a Bear). Output is a flat
``{tag: value}`` dict; tags are logged verbatim under the run and carried in ``EvalResult.analyzers``.
"""

from __future__ import annotations

from typing import Callable, Protocol, runtime_checkable

import numpy as np


@runtime_checkable
class Analyzer(Protocol):
    name: str

    def observe(self, obs: "dict[str, np.ndarray]", record: dict) -> None: ...
    def result(self) -> "dict[str, float]": ...
    def reset(self) -> None: ...


# ── obs layout (obs.rs; absolute indices, append-stable) ─────────────────────────────────────────
_G_MY_LIFE = 16                   # globals: my life
_G_DECISION0 = 43                 # globals: decision-kind one-hot base (obs.rs::encode_globals)
_R_DECL_ATTACKERS = 9             # request_index(DeclareAttackers)
_R_DECL_BLOCKERS = 10             # request_index(DeclareBlockers)
# bf_feat per-permanent columns. 0-43 are append-stable; is_pending_combat is the new last (44).
_BF_PRESENT, _BF_MINE, _BF_POWER = 0, 1, 2
_BF_TAPPED = 6
_BF_ATTACKING, _BF_BLOCKED_BY, _BF_PENDING = 39, 43, 44
_SWINE_POWER, _BEAR_POWER = 3, 2  # Argothian Swine (3/3 trample) vs Grizzly Bears (2/2)


def _decision_is(obs, ridx: int) -> bool:
    g = np.asarray(obs["globals"]).ravel()
    return g[_G_DECISION0 + ridx] > 0.5


class SwineBlockAnalyzer:
    """Combat-judgment signals for the swine deck (bears 2/2 vs trampling swine 3/3).

    The user's ground truths, encoded exactly:
      * **chump_block_rate** — it is NEVER correct to chump-block the swine with a lone bear UNLESS not
        blocking would be lethal. Fraction of (NOT-forced) swine-attacks answered by a lone-bear block.
        Forced (taking the unblocked total would put you ≤0) chumps are counted in a SEPARATE bucket so
        they don't pollute it.
      * **double_block_rate** — double-blocking the swine with two bears IS correct (4 power into 3
        toughness). Fraction of swine-attacks answered by a 2+ blocker gang on the swine.
      * **lone_bear_attack_rate** — it is NEVER correct to attack with a single bear into an untapped
        enemy swine (the bear just dies). Fraction of attack decisions (with an untapped enemy swine
        able to block) that send exactly one bear.

    Also keeps the legacy high-life block signals (``chump_rate_swine_hi``/``gang_rate_swine_hi``) for
    dashboard continuity. All classification is off the obs board + the decision-kind one-hot.
    """

    name = "swine"

    def __init__(self, life_hi: int = 15):
        self.life_hi = int(life_hi)
        self.reset()

    def reset(self) -> None:
        # one row per swine-attack (DeclareBlockers with a swine attacking):
        #   (life, blocked, gang_legacy, forced, lone_bear_chump, double_block)
        self._blk: "list[tuple]" = []
        # one row per attack decision with an untapped enemy swine present: (lone_bear_attack,)
        self._atk: "list[float]" = []

    # ── observe ──────────────────────────────────────────────────────────────────────────────────
    def observe(self, obs, record) -> None:
        if not record:
            return
        bf = np.asarray(obs["bf_feat"])
        present = bf[:, _BF_PRESENT] > 0.5
        mine = present & (bf[:, _BF_MINE] > 0.5)
        enemy = present & (bf[:, _BF_MINE] < 0.5)

        if record.get("block_eligible", 0) > 0 and _decision_is(obs, _R_DECL_BLOCKERS):
            self._observe_block(obs, record, bf, mine, enemy)
        elif record.get("attack_eligible", 0) > 0 and _decision_is(obs, _R_DECL_ATTACKERS):
            self._observe_attack(bf, mine, enemy)

    def _observe_block(self, obs, record, bf, mine, enemy):
        enemy_attacking = enemy & (bf[:, _BF_ATTACKING] > 0.5)
        swine = enemy_attacking & (bf[:, _BF_POWER] == _SWINE_POWER)
        if not swine.any():
            return  # only score decisions where a Swine is actually attacking
        life = float(np.asarray(obs["globals"]).ravel()[_G_MY_LIFE])
        # Forced = taking ALL unblocked incoming would put me to ≤0 (the user's exception to "no chump").
        total_incoming = float(bf[enemy_attacking, _BF_POWER].sum())
        forced = total_incoming >= life
        # My blockers assigned THIS decision (is_pending_combat), by power.
        my_pending = mine & (bf[:, _BF_PENDING] > 0.5)
        pending_powers = bf[my_pending, _BF_POWER]
        n_bears = int((pending_powers == _BEAR_POWER).sum())
        n_swine = int((pending_powers == _SWINE_POWER).sum())
        swine_blocked_by = bf[swine, _BF_BLOCKED_BY]
        double_block = bool((swine_blocked_by >= 2).any())        # a swine ganged by 2+
        lone_on_swine = bool((swine_blocked_by == 1).any())       # a swine blocked by exactly one
        # Lone-bear chump: a swine is lone-blocked and the lone blocker is a bear. Unambiguous when the
        # only pending blockers are bears; if a pending swine-blocker exists the lone blocker's identity
        # is ambiguous (could be a fair swine-trade), so we don't count it (conservative).
        lone_bear_chump = lone_on_swine and n_bears >= 1 and n_swine == 0
        blocked = record.get("block_declared", 0) > 0
        self._blk.append((
            life,
            1.0 if blocked else 0.0,
            1.0 if record.get("block_double", 0) > 0 else 0.0,
            1.0 if forced else 0.0,
            1.0 if lone_bear_chump else 0.0,
            1.0 if double_block else 0.0,
        ))

    def _observe_attack(self, bf, mine, enemy):
        # "untapped enemy swine could block" — an enemy 3-power creature that is untapped.
        enemy_swine_untapped = enemy & (bf[:, _BF_POWER] == _SWINE_POWER) & (bf[:, _BF_TAPPED] < 0.5)
        if not enemy_swine_untapped.any():
            return  # only score attack decisions where a Swine could block
        # My declared attackers this decision (is_pending_combat on the attack step).
        my_attackers = mine & (bf[:, _BF_PENDING] > 0.5)
        powers = bf[my_attackers, _BF_POWER]
        lone_bear_attack = (powers.size == 1) and (powers[0] == _BEAR_POWER)
        self._atk.append(1.0 if lone_bear_attack else 0.0)

    # ── result ──────────────────────────────────────────────────────────────────────────────────
    def result(self) -> "dict[str, float]":
        b = np.array(self._blk) if self._blk else np.empty((0, 6))
        a = np.array(self._atk) if self._atk else np.empty((0,))
        out = {
            "swine/n_swine_attacks": float(len(b)),
            "swine/n_attack_decisions": float(len(a)),
        }
        if len(b):
            life, blocked, gang, forced, chump, double = (b[:, i] for i in range(6))
            not_forced = forced < 0.5
            out["swine/n_forced_swine_attacks"] = float(forced.sum())
            # The user's headline metric: lone-bear chump of the swine when NOT forced.
            out["swine/chump_block_rate"] = (float(chump[not_forced].mean())
                                             if not_forced.any() else float("nan"))
            # Forced chumps kept separate (defensible — chumping to survive lethal is correct).
            out["swine/chump_block_rate_forced"] = (float(chump[forced > 0.5].mean())
                                                    if (forced > 0.5).any() else float("nan"))
            # Double-blocking the swine (the correct sophisticated line) over all swine-attacks.
            out["swine/double_block_rate"] = float(double.mean())
            # Legacy high-life signals (dashboard continuity).
            hi = life >= self.life_hi
            swine_hi = hi  # every row here already has a swine attacking
            out["swine/chump_rate_swine_hi"] = (float(blocked[swine_hi].mean())
                                                if swine_hi.any() else float("nan"))
            bm = swine_hi & (blocked > 0.5)
            out["swine/gang_rate_swine_hi"] = float(gang[bm].mean()) if bm.any() else float("nan")
        if len(a):
            out["swine/lone_bear_attack_rate"] = float(a.mean())
        return out


# ── registry ─────────────────────────────────────────────────────────────────────────────────────
ANALYZERS: "dict[str, Callable[[], Analyzer]]" = {
    "swine": SwineBlockAnalyzer,
}


def register_analyzer(deck: str, factory: "Callable[[], Analyzer]") -> None:
    """Register (or override) the analyzer factory for ``deck``."""
    ANALYZERS[deck] = factory


def get_analyzer(deck: str) -> "Analyzer | None":
    """The analyzer for ``deck`` (a fresh instance), or ``None`` if the deck has none."""
    factory = ANALYZERS.get(deck)
    return factory() if factory is not None else None
