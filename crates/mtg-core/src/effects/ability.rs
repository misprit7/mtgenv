//! Ability kinds — the rule registry of the whiteboard model (WHITEBOARD_MODEL.md §2.3). Every
//! card ability is one of these, all *data* interpreted by the effect runtime: spell,
//! activated (incl. mana), triggered (incl. delayed/state), replacement/prevention, and
//! continuous/static (which contribute to layers + qualifications).

use super::condition::{Condition, Duration};
use super::target::{CardFilter, CardType, SelectSpec};
use super::value::ValueExpr;
use super::Effect;
use crate::basics::{Color, CounterKind, DamageKind, ManaCost};
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
    /// An additional mana payment beyond the base cost.
    AdditionalMana(ManaCost),
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
    /// This creature blocks or becomes blocked (CR 509.1i).
    SelfBlocks,
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
    WouldAddCounters(CounterKind),
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
    /// Enter tapped.
    EntersTapped,
}

/// A continuous/static effect's contribution to the layer system (CR 613) and/or a
/// qualification it paints on objects (WHITEBOARD_MODEL.md §2.4). Pure data (no `Effect`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StaticContribution {
    /// Layer 7c: modify power/toughness (a `+N/+N` style effect).
    ModifyPT { power: i32, toughness: i32 },
    /// Layer 7b: set base power/toughness to specific values.
    SetBasePT { power: i32, toughness: i32 },
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
    /// A continuous/static effect (CR 604/611/613): contributes to a layer and/or paints a
    /// qualification, for the given duration over the given affected set.
    Static {
        contribution: StaticContribution,
        affects: SelectSpec,
        duration: Duration,
    },
}
