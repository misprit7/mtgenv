//! Dynamic values and player references used throughout the Effect IR. A `ValueExpr` is a
//! number that may read game state at resolution time (CR 608.2h — info read once, at
//! application); a `PlayerRef` names a player relative to the effect's source/controller.

use super::target::CardFilter;
use crate::basics::{CounterKind, Zone};
use serde::{Deserialize, Serialize};

/// A player named relative to the resolving effect. Resolved against the `ResolutionCtx` when
/// an `Action` is materialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlayerRef {
    /// The controller of the effect's source (the usual "you").
    Controller,
    /// The single opponent (2-player). Generalizes to "each opponent" via `EachOpponent`.
    Opponent,
    EachOpponent,
    EachPlayer,
    /// The owner of the effect's source.
    Owner,
    /// A player chosen as the Nth target of this effect.
    ChosenTarget(u32),
    /// The player currently bound by an enclosing [`Effect::ForEachTarget`] over a **player** slot
    /// (the player analogue of [`crate::effects::EffectTarget::Each`], which reads the same
    /// `foreach_current` cursor). Lets a per-iteration body name "that player" — e.g. "any number of
    /// target players each discard a card" = `ForEachTarget{ slot: player, body: Discard{ who: Each } }`
    /// (Ral Zarek, Guest Lecturer). Resolves to the source's controller if used outside such a loop
    /// (or if the current binding isn't a player).
    Each,
    /// The *controller* of the Nth (object) target of this effect, snapshotted at resolution
    /// start — so it survives that object leaving play during the same resolution (e.g. Erode's
    /// "Destroy target creature. Its controller may search…", where "its controller" is read
    /// before the destroy moves it to the graveyard). CR 608.2 (info read as the effect resolves).
    ControllerOfTarget(u32),
    /// The controller of the object currently bound by an enclosing `ForEach` (CR "its controller",
    /// per-iteration) — the object analogue of [`Each`]. Used by an overloaded spell's per-object rider
    /// (Winds of Abandon: "for each creature exiled this way, its controller searches …"). Falls back to
    /// the effect's controller outside a `ForEach` or if the current binding isn't an object.
    ControllerOfEach,
}

/// A number that may be fixed or computed from game state. Kept small; grows with the IR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueExpr {
    /// A literal amount.
    Fixed(i64),
    /// `yes` if the resolving spell was **kicked** (CR 702.33, [`crate::state::Object::kicked`] on
    /// `ctx.source`), else `no` — Burst Lightning's "deals 2 damage; if kicked, 4 instead."
    IfKicked { yes: Box<ValueExpr>, no: Box<ValueExpr> },
    /// The value of X chosen at cast/activation (CR 107.3).
    X,
    /// A fixed multiple of X (e.g. "twice X").
    XTimes(i64),
    /// The number of targets this effect has.
    NumTargets,
    /// Count objects in a zone matching a filter, optionally restricted by controller.
    Count {
        zone: Zone,
        filter: CardFilter,
        controller: Option<PlayerRef>,
    },
    /// The number of **distinct names** (CR 201.2) among objects in a zone matching a filter,
    /// optionally restricted by controller — "the number of differently named lands you control"
    /// (Emil). Like [`Count`], but deduplicates by card name before counting.
    DistinctNames {
        zone: Zone,
        filter: CardFilter,
        controller: Option<PlayerRef>,
    },
    /// Sum of `a` and `b` (composition so simple arithmetic is expressible without new nodes).
    Sum(Box<ValueExpr>, Box<ValueExpr>),
    /// `2` raised to the (non-negative) inner value — "draws 2ˣ cards" (Mathemagics: `Pow2(X)`, so
    /// 2⁰=1, 2¹=2, 2²=4, …). The exponent is clamped to `[0, 62]` before shifting (an exponent beyond a
    /// real deck size decks the player out regardless), so it never overflows `i64`.
    Pow2(Box<ValueExpr>),
    /// Half the inner value, **rounded down** (`inner / 2`, integer division) — "half their life / half
    /// the cards in their hand / half the permanents they control" (Pox Plague, "round down each time").
    /// A generic halving node (evergreen in MTG); pairs with [`LifeTotal`] / [`HandSize`] / [`Count`] under
    /// a [`super::Effect::ForEachPlayer`] loop that binds [`PlayerRef::Each`] to the iterated player.
    Half(Box<ValueExpr>),
    /// The current life total of `who` (CR 119) — reads `Player.life`. "Loses half their life" (Pox
    /// Plague) = `Half(LifeTotal { who: Each })` under a per-player loop. The life analogue of [`HandSize`].
    LifeTotal { who: PlayerRef },
    /// Total **toughness** of the battlefield permanents matching `filter`, optionally restricted by
    /// controller — "creatures you control have total toughness 10 or greater" (Orysa's cost
    /// reduction, via `Condition::ValueAtLeast`). Sums computed toughness; `None`-toughness = 0.
    TotalToughness {
        filter: CardFilter,
        controller: Option<PlayerRef>,
    },
    /// The **greatest mana value** (CR 202.3) among battlefield objects matching `filter`, optionally
    /// restricted by controller — "the greatest mana value among creatures and planeswalkers they
    /// control" (End of the Hunt). `0` if none match. Used dynamically (via `CardFilter::ManaValueExpr`)
    /// to select the max-MV object.
    GreatestManaValue {
        filter: CardFilter,
        controller: Option<PlayerRef>,
    },
    /// The **computed power** of the effect's source object at resolution (CR 613) — used by the SoS
    /// "Increment" check ("mana spent > this creature's power or toughness"). `0` if no source.
    PowerOfSelf,
    /// The **computed toughness** of the effect's source object at resolution. `0` if no source.
    ToughnessOfSelf,
    /// The number of counters of `kind` on **this object** — the resolving effect's source at
    /// resolution time, or the object being computed in a layer-7a CDA (`SetBasePTValue`). Used
    /// for "P/T = the number of +1/+1 counters on it" and "double the counters on this" effects.
    CountersOnSelf(CounterKind),
    /// The **computed power** of the Nth chosen target, snapshotted at resolution (CR 608.2h). For
    /// "double its power" (Mightform): `PumpPT{ power: PowerOfTarget(0), toughness: Fixed(0) }`
    /// adds the target's current power to itself. `0` if the target isn't an object on the
    /// battlefield.
    PowerOfTarget(u32),
    /// The total **mana spent to cast** the Nth chosen target (a spell on the stack), read at
    /// resolution — `Object.mana_spent` (incl. any `{X}`). For "add {C} equal to the amount of mana
    /// spent to cast that spell" (Mana Sculpt), read while the countered spell is still on the stack.
    /// `0` if the target isn't an object.
    ManaSpentOfTarget(u32),
    /// The **mana value** (CR 202.3) of the Nth chosen target, read at resolution. Mana value is a
    /// printed/copiable characteristic (from the card's mana cost), so it's stable across a zone move
    /// in the same resolution — used by "you lose life equal to that card's mana value" (Reanimate),
    /// which reads the reanimated creature card's MV. `0` if the target isn't an object.
    ManaValueOfTarget(u32),
    /// The number of `kind` counters on the Nth chosen target, read at resolution. Unlike a value
    /// snapshotted once, this reads **live** state — so a prior counter-adding step in the same
    /// resolution is visible IF it committed first. The `PutCounters` interpret arm flushes staged
    /// actions before it runs, so "put a +1/+1 counter on target creature, then double the number of
    /// +1/+1 counters on it" (Growth Curve) reads the post-first-counter count. `0` if the target
    /// isn't an object.
    CountersOnTarget { target: u32, kind: CounterKind },
    /// The total mana spent to cast the effect's source object (CR 601.2f–h, incl. any `{X}`),
    /// recorded as it was cast and read when it enters. For "enters with +1/+1 counters equal to
    /// the mana spent to cast it" (Dyadrine). `0` if the source wasn't cast (token / put onto the
    /// battlefield).
    ManaSpent,
    /// The number of **distinct colours of mana spent** to cast the effect's source object (CR 702.75
    /// Converge), recorded as it was cast and read when it enters/resolves. For the SoS "Archaic"
    /// cycle ("enters with a +1/+1 counter for each color of mana spent to cast it"). `0` if the source
    /// wasn't cast.
    ColorsSpent,
    /// The total mana spent to cast the **triggering spell** of a "whenever you cast …" ability (the
    /// SoS "Opus" cycle) — read from `ResolutionCtx::triggering_spell`. `0` outside such a trigger.
    ManaSpentOnTrigger,
    /// The number of **distinct colours of mana spent** to cast the **triggering spell** of a "whenever
    /// you cast …" ability (Converge on a cast-trigger — Magmablood Archaic's "for each color of mana
    /// spent to cast that spell") — read from `ResolutionCtx::triggering_spell`. `0` outside such a
    /// trigger. The colours-of-trigger analogue of [`ColorsSpent`] / [`ManaSpentOnTrigger`].
    ColorsSpentOnTrigger,
    /// The number of **distinct card types** among the cards in exile that were exiled *with* the
    /// effect's source object (`Object.exiled_with == source`) — Keen-Eyed Curator's "four or more
    /// card types among cards exiled with this creature." `0` if there's no source.
    DistinctCardTypesAmongExiledWith,
    /// The number of cards the effect's controller has **drawn this turn** (CR 120) — the SoS Quandrix
    /// "X = the number of cards you've drawn this turn" (Fractal Anomaly). Reads
    /// `Player.cards_drawn_this_turn` for `ctx.controller`.
    CardsDrawnThisTurn,
    /// The number of cards discarded **so far during the current resolution** (CR 608) — reads the
    /// engine's per-resolution discard scratch. For "then draw cards equal to the number of cards
    /// discarded this way" (Borrowed Knowledge) / "draw that many cards plus one" (Colossus of the
    /// Blood Age, via `Sum(_, 1)`). `0` outside a discard resolution.
    DiscardedThisResolution,
    /// The number of permanents **destroyed so far during the current resolution** (CR 608) — reads
    /// the engine's per-resolution destroy scratch (only actual destructions, not indestructible /
    /// replaced-away ones). For "add {B} or {G} for each permanent destroyed this way" (Culling
    /// Ritual), read after the mass-destroy step commits. `0` outside a destroy resolution.
    DestroyedThisResolution,
    /// The value chosen for `{X}` (CR 107.3) of the **triggering spell** of a "whenever you cast a
    /// spell with {X} in its mana cost" ability — read from `ResolutionCtx::triggering_spell`'s
    /// `Object.cast_x`. "Look at the top X cards" (Geometer's Arthropod). `0` outside such a trigger
    /// or if the triggering spell had no `{X}`.
    XOfTriggeringSpell,
    /// Total life `who` has **gained this turn** (CR 119) — reads `Player.life_gained_this_turn`. For
    /// a "if you gained N or more life this turn" gate (Scheming Silvertongue), via `ValueAtLeast`.
    LifeGainedThisTurn { who: PlayerRef },
    /// The number of separate life-gain **events** `who` has had this turn — reads
    /// `Player.life_gain_events_this_turn`. For "whenever you gain life for the first time each turn"
    /// (Leech Collector): the trigger gates on this being exactly 1 at queue time.
    LifeGainEventsThisTurn { who: PlayerRef },
    /// The number of **creatures that died this turn** (any controller) — summed across players'
    /// `Player.creatures_died_this_turn`. For a "if two or more creatures died this turn" gate
    /// (Emeritus of Woe), via `ValueAtLeast`.
    CreaturesDiedThisTurn,
    /// The number of cards **put into exile this turn** (any owner) — summed across players'
    /// `Player.cards_exiled_this_turn`. For "if one or more cards were put into exile this turn"
    /// (Ennis, Debate Moderator), via `ValueAtLeast`.
    CardsExiledThisTurn,
    /// The number of cards in `who`'s hand (CR 402). For an "if an opponent has more cards in hand
    /// than you" gate (Joined Researchers): `ValueAtLeast(HandSize(Opponent), Sum(HandSize(Controller), 1))`.
    HandSize { who: PlayerRef },
    /// The number of spells (any type) `who` has cast this turn — `Player.spells_cast_this_turn`. For a
    /// "whenever you cast your third spell each turn" gate (Emeritus of Conflict): an *exactly-N* check
    /// via `All(ValueAtLeast(v, N), Not(ValueAtLeast(v, N+1)))`.
    SpellsCastThisTurn { who: PlayerRef },
    /// The number of instant/sorcery spells `who` has cast this turn — `Player.instants_sorceries_cast_this_turn`.
    /// The counter increments at cast (601.2i), so a resolving I/S spell counts *itself* here — an
    /// "if you've cast **another** instant or sorcery this turn" gate (Burrog Barrage) is therefore
    /// `ValueAtLeast(InstantsSorceriesCastThisTurn{Controller}, 2)`. The value analogue of the boolean
    /// [`super::condition::Condition::CastInstantOrSorceryThisTurn`] (≥1).
    InstantsSorceriesCastThisTurn { who: PlayerRef },
}

impl ValueExpr {
    /// Convenience: a literal.
    pub fn lit(n: i64) -> Self {
        ValueExpr::Fixed(n)
    }
}
