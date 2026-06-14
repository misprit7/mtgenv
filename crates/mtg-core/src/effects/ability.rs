//! Ability kinds — the rule registry of the whiteboard model (WHITEBOARD_MODEL.md §2.3). Every
//! card ability is one of these, all *data* interpreted by the effect runtime: spell,
//! activated (incl. mana), triggered (incl. delayed/state), replacement/prevention, and
//! continuous/static (which contribute to layers + qualifications).

use super::condition::{Condition, Duration};
use super::target::{CardFilter, SelectSpec};
use super::value::ValueExpr;
use super::Effect;
use crate::basics::{CardType, Color, CounterKind, DamageKind, ManaCost};
use serde::{Deserialize, Serialize};

/// A cost to be paid (CR 118): an optional mana component plus any number of non-mana
/// components. `{0}`/0-life are always payable (CR 118.3a/b).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cost {
    pub mana: Option<ManaCost>,
    pub components: Vec<CostComponent>,
}

/// A non-mana cost component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostComponent {
    /// `{T}` — tap the source.
    TapSelf,
    /// `{Q}` — untap the source.
    UntapSelf,
    /// Sacrifice permanents matching the spec.
    Sacrifice(SelectSpec),
    /// Pay life.
    PayLife(ValueExpr),
    /// Discard cards matching the spec.
    Discard(SelectSpec),
    /// Exile cards matching the spec (e.g. for escape/delve).
    Exile(SelectSpec),
    /// Remove counters from the source.
    RemoveCounters { kind: CounterKind, n: ValueExpr },
    /// Crew N (CR 702.122): tap any number of untapped creatures you control with total power ≥ N.
    Crew(u32),
    /// An additional mana payment beyond the base cost.
    AdditionalMana(ManaCost),
    /// A planeswalker loyalty-ability cost (CR 606.2): `+N` adds N loyalty counters, `−N`
    /// removes N, `0` neither. Payable iff `n >= 0` or the source has at least `-n` loyalty
    /// counters (you can't pay a `−N` you don't have). The once-per-turn limit on loyalty
    /// abilities is **per planeswalker, across all its loyalty abilities** (606.3) — enforced
    /// by the engine, not by this cost.
    Loyalty(i32),
}

/// Timing restriction for casting/activating (CR 117.1a, 602.5d/e).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Timing {
    Instant,
    Sorcery,
}

/// Extra activation restrictions beyond timing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Restriction {
    OncePerTurn,
    OnlyYourTurn,
    /// Only if a condition holds.
    OnlyIf(Condition),
}

/// The event a triggered ability watches for (CR 603.2). A starter vocabulary; grows with the
/// card pool. "Self" means the ability's own source object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventPattern {
    /// This permanent entered the battlefield (ETB, CR 603.6a).
    SelfEnters,
    /// This permanent left the battlefield (LTB).
    SelfLeaves,
    /// This permanent died (went to graveyard from the battlefield).
    SelfDies,
    /// Any permanent matching the filter entered the battlefield.
    PermanentEnters(CardFilter),
    /// A creature matching the filter died.
    CreatureDies(CardFilter),
    /// A spell matching the filter was cast (CR 601.2i).
    SpellCast(CardFilter),
    /// Damage was dealt (optionally of a given kind) to a matching object/player.
    DamageDealt { kind: Option<DamageKind> },
    /// The beginning of a step/phase (CR 500.6, a triggered — not turn-based — ability).
    BeginningOfStep(crate::basics::Phase),
    /// This creature attacks (CR 508.1m).
    SelfAttacks,
    /// You attack — one or more creatures you control are declared as attackers (CR 508.1, a
    /// once-per-combat "whenever you attack …" trigger for the attacking player). Distinct from
    /// `SelfAttacks` (per-attacking-creature): fires once for the watcher whose controller attacked.
    YouAttack,
    /// This creature blocks or becomes blocked (CR 509.1i).
    SelfBlocks,
    /// You tap a creature for mana (CR 605.1b) — drives "whenever you tap a creature for mana, add
    /// …" no-stack triggered mana abilities (Badgermole Cub). Fires per creature tapped for mana.
    TapCreatureForMana,
    /// A permanent matching `filter` (relative to the watcher's controller) becomes the target of a
    /// spell or ability (CR 603.2/603.3d, fired when targets are locked). `by_opponent` restricts to
    /// targeting sources controlled by an opponent of the watcher (Surrak: "a creature you control
    /// becomes the target of a spell or ability an opponent controls").
    BecomesTargeted { filter: CardFilter, by_opponent: bool },
}

/// What an `Action` pattern matches, for the replacement/prevention rewrite pass (CR 614/615).
/// The pass consults these against pending whiteboard `Action`s (WHITEBOARD_MODEL.md §2.1 step 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionPattern {
    WouldBeDestroyed(CardFilter),
    WouldBeDealtDamage { to: CardFilter, kind: Option<DamageKind> },
    WouldDraw,
    WouldGainLife,
    WouldLoseLife,
    WouldEnterBattlefield(CardFilter),
    /// Counters of `kind` would be put on an object matching `to` (CR 614.1, e.g. Hardened
    /// Scales / Doubling Season modify "+1/+1 counters on a creature you control" — the `to`
    /// filter is the affected-object scope, often `ControlledBy(Controller)` or `ItSelf`).
    WouldAddCounters { kind: CounterKind, to: CardFilter },
}

/// How a matched action is rewritten (CR 614.1). Contains an `Effect` in the "instead" case, so
/// this type is not serde-serializable (card data, not snapshot state).
#[derive(Debug, Clone)]
pub enum Rewrite {
    /// Delete the action entirely (prevention / "can't").
    Prevent,
    /// Skip (for "skip your draw step"-style — a deletion at a higher level).
    Skip,
    /// Replace the event with a different effect ("instead").
    ReplaceWith(Box<Effect>),
    /// Modify the action's amount (e.g. damage doublers/reducers): new = f(old).
    ScaleAmount { numerator: u32, denominator: u32 },
    /// Add to the action's amount.
    AddAmount(i64),
    /// Redirect damage/effect to a different recipient (the controller chooses if ambiguous).
    Redirect,
    /// Enter with N extra counters of a kind (the common ETB replacement, CR 614.1e).
    EntersWithCounters { kind: CounterKind, n: u32 },
    /// Enter with a **dynamic** number of counters — `n` is evaluated as the object enters, against
    /// the entering object as the source (CR 614.1e). For "enters with +1/+1 counters equal to the
    /// mana spent to cast it" (Dyadrine): `n = ValueExpr::ManaSpent`.
    EntersWithCountersValue { kind: CounterKind, n: ValueExpr },
    /// Enter tapped.
    EntersTapped,
    /// Enter tapped **unless** the condition holds — the "check land" pattern ("enters tapped
    /// unless you control a basic land"). No choice; evaluated for the controller as it enters.
    EntersTappedUnless(Condition),
    /// Enter tapped **unless** the controller pays `life` life — the "shock land" pattern ("you
    /// may pay 2 life; if you don't, it enters tapped"). The controller is asked as it enters.
    EntersTappedUnlessPay { life: u32 },
}

/// A continuous/static effect's contribution to the layer system (CR 613) and/or a
/// qualification it paints on objects (WHITEBOARD_MODEL.md §2.4). Pure data (no `Effect`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaticContribution {
    /// Layer 7c: modify power/toughness (a `+N/+N` style effect).
    ModifyPT { power: i32, toughness: i32 },
    /// Layer 7b: set base power/toughness to specific values.
    SetBasePT { power: i32, toughness: i32 },
    /// Layer 7a CDA (CR 604.3/613.4b): set base P/T from **dynamic** values, evaluated against the
    /// object being computed — e.g. Lumbering Worldwagon `*/4` (`*` = lands you control), or a
    /// creature whose P/T equals its +1/+1 counters (`CountersOnSelf`).
    SetBasePTValue { power: ValueExpr, toughness: ValueExpr },
    /// Layer 6: grant a keyword ability.
    GrantKeyword(Keyword),
    /// Layer 6: remove a keyword ability.
    RemoveKeyword(Keyword),
    /// Layer 4: add a card type.
    AddType(CardType),
    /// Layer 5: set/add color.
    AddColor(Color),
    SetColor(Vec<Color>),
    /// A qualification marker the structural machinery respects (CR 613/§2.4).
    Qualification(Qualification),
    /// A generic cost reduction (CR 118.7) — reduces generic mana by N.
    CostReductionGeneric(u32),
}

/// Evergreen keyword abilities (CR 702) — the starter set. Grows with the card pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Keyword {
    Deathtouch,
    Defender,
    DoubleStrike,
    FirstStrike,
    Flash,
    Flying,
    Haste,
    Hexproof,
    Indestructible,
    Lifelink,
    Menace,
    Reach,
    Trample,
    Vigilance,
    Ward,
}

/// A boolean/typed marker painted on objects by the layer/qualification pass; the whiteboard
/// rewrite pass and legality checks read these instead of abilities intercepting actions
/// directly (WHITEBOARD_MODEL.md §2.4 — MTGA's exact trick).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Qualification {
    Indestructible,
    Hexproof,
    Shroud,
    CantBeSacrificed,
    CantAttack,
    MustAttack,
    CantBlock,
    /// This creature can't be blocked (CR 509.1b) — combat reads it on the attacker. Escape Tunnel.
    CantBeBlocked,
    CantBeCountered,
    PhasedOut,
}

/// A card ability — one of the five functional kinds (CR 113.3). Contains `Effect`/`Rewrite`
/// trees, so it derives only `Debug`/`Clone` (card *data*, not snapshot state).
#[derive(Debug, Clone)]
pub enum Ability {
    /// An instant/sorcery spell ability (CR 113.3a / 608): resolve its effect.
    Spell { effect: Effect },
    /// `Cost: Effect` (CR 602). `is_mana` marks the no-stack mana-ability subset (CR 605).
    Activated {
        cost: Cost,
        effect: Effect,
        timing: Timing,
        restriction: Option<Restriction>,
        is_mana: bool,
    },
    /// `When/Whenever/At [event] [, if cond]: Effect` (CR 603). `intervening_if` marks the
    /// 603.4 double-check semantics for `condition`.
    Triggered {
        event: EventPattern,
        condition: Option<Condition>,
        intervening_if: bool,
        effect: Effect,
    },
    /// `[action pattern] -> rewrite` for the rewrite pass (CR 614/615).
    Replacement {
        pattern: ActionPattern,
        rewrite: Rewrite,
    },
    /// Warp (CR 702.x): "You may cast this card from your hand for its warp `cost`. Exile it at the
    /// beginning of the next end step, then you may cast it from exile on a later turn." A static
    /// casting-permission ability — `legal_priority_actions` scans for it to offer the alternative
    /// cast; the engine handles the exile-at-end-step + cast-from-exile mechanics.
    Warp { cost: ManaCost },
    /// A continuous/static effect (CR 604/611/613): contributes to a layer and/or paints a
    /// qualification, for the given duration over the given affected set.
    Static {
        contribution: StaticContribution,
        affects: SelectSpec,
        duration: Duration,
    },
    /// A static effect that applies **only while `condition` holds** (CR 604.3) — e.g. Keen-Eyed
    /// Curator's "+4/+4 and trample as long as there are four or more card types among cards exiled
    /// with this creature." The condition is evaluated relative to the source permanent each
    /// recompute, so the contribution toggles on/off; otherwise identical to [`Ability::Static`].
    ConditionalStatic {
        contribution: StaticContribution,
        affects: SelectSpec,
        duration: Duration,
        condition: Condition,
    },
}
