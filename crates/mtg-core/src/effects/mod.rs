//! The Effect IR — card behaviour as **data** interpreted by the effect runtime (the "CLIPS"
//! layer of the whiteboard model). Resolving an effect *materializes* a `Whiteboard` of atomic
//! `Action`s (WHITEBOARD_MODEL.md §2.1/2.3). The core engine must never `match` on card
//! identity — all card-specific behaviour is one of these nodes, with the `Native` escape hatch
//! for the genuinely-unique.
//!
//! Submodules:
//! - [`action`]  — the whiteboard `Action` vocabulary + `Whiteboard`/`ResolutionCtx`.
//! - [`ability`] — the five ability kinds (spell/activated/triggered/replacement/static) + costs.
//! - [`value`]   — `ValueExpr` (dynamic numbers) + `PlayerRef`.
//! - [`target`]  — target/selection criteria, `CardFilter`, `TokenSpec`, `ManaSpec`.
//! - [`condition`] — `Condition` (intervening-if) + `Duration`.
//! - [`native`]  — the `Native` escape hatch (`EffectCtx` + `NativeFn`).

pub mod ability;
pub mod action;
pub mod condition;
pub mod native;
pub mod target;
pub mod value;

use self::ability::Keyword;
use self::condition::{Condition, Duration};
use self::native::NativeFn;
use self::target::{CardFilter, ManaSpec, SelectSpec, TargetSpec, TokenCopyMods, TokenSpec};
use self::value::{PlayerRef, ValueExpr};
use crate::basics::{CounterKind, DamageKind, Zone, ZoneDest};

/// How an effect leaf refers to the thing(s) it acts on. A leaf either acts on a target locked
/// at cast (`Target`, CR 601.2c), a set selected at resolution (`Select`), a named player, the
/// source itself, or an already-chosen target slot by index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectTarget {
    /// "target …" — chosen and locked when the spell/ability is put on the stack (601.2c).
    Target(TargetSpec),
    /// A set chosen at resolution without the word "target".
    Select(SelectSpec),
    /// A player named relative to the source.
    Player(PlayerRef),
    /// The effect's own source object.
    SourceSelf,
    /// The Nth target already chosen for this spell/ability.
    ChosenIndex(u32),
    /// The Nth permanent found by a `Search` earlier in this same resolution — "that land/creature"
    /// (Fabled Passage's "untap that land"). No targeting; it's whatever was fetched.
    Searched(u32),
    /// The object currently being iterated by the enclosing `ForEach` (CR "for each …") — used by a
    /// per-iteration body, e.g. "remove a counter from each of two creatures you control."
    Each,
}

/// One mode of a modal spell/ability (CR 700.2): a presented label + the effect it runs.
#[derive(Debug, Clone)]
pub struct Mode {
    pub label: String,
    pub effect: Effect,
}

/// How long an impulse-exiled card stays playable (SoS impulse-play, `Effect::ExileForPlay`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayWindow {
    /// "until end of turn" — playable through the current turn only (e.g. Tablet of Discovery).
    ThisTurn,
    /// "until the end of your next turn" — through the owner's next turn (e.g. Practiced Scrollsmith).
    YourNextTurn,
}

/// The effect vocabulary. Leaves lower to `Action`s; interior nodes are control flow and choice
/// points (each choice point becomes a `DecisionRequest` — see `agent`). Contains `NativeFn`
/// function pointers, so it derives `Debug`/`Clone` but not `serde` (card data, not state).
#[derive(Debug, Clone)]
pub enum Effect {
    // ── leaves: lower to Action(s) ──────────────────────────────────────────────
    DealDamage {
        amount: ValueExpr,
        to: EffectTarget,
        kind: DamageKind,
    },
    Draw {
        who: PlayerRef,
        count: ValueExpr,
    },
    Destroy {
        what: EffectTarget,
    },
    Sacrifice {
        who: PlayerRef,
        what: SelectSpec,
    },
    Mill {
        who: PlayerRef,
        count: ValueExpr,
    },
    /// Surveil N (CR 701.42): look at the top `count` cards of your library, then put any number of
    /// them into your graveyard and the rest back on top in any order. The controller chooses which
    /// go to the graveyard (a resolution-time decision, so it's driven imperatively).
    Surveil {
        count: ValueExpr,
    },
    GainLife {
        who: PlayerRef,
        amount: ValueExpr,
    },
    LoseLife {
        who: PlayerRef,
        amount: ValueExpr,
    },
    PumpPT {
        what: EffectTarget,
        power: ValueExpr,
        toughness: ValueExpr,
        duration: Duration,
    },
    /// Grant a keyword to the target for a duration (CR 611) — e.g. "it gains trample until end of
    /// turn." Lowers to a floating `GrantContinuous{ GrantKeyword(keyword), duration }`.
    GrantKeyword {
        what: EffectTarget,
        keyword: Keyword,
        duration: Duration,
    },
    /// Paint a qualification on the target for a duration (CR 611 / §2.4) — e.g. "target creature
    /// can't be blocked this turn." Lowers to `GrantContinuous{ Qualification(q), duration }`.
    GrantQualification {
        what: EffectTarget,
        qualification: self::ability::Qualification,
        duration: Duration,
    },
    /// The target becomes a creature for a duration (CR 611) — a crewed Vehicle "becomes an artifact
    /// creature until end of turn." Lowers to `GrantContinuous{ AddType(Creature), duration }` (its
    /// P/T comes from its own characteristics/CDA; it's already an artifact).
    BecomeCreature {
        what: EffectTarget,
        duration: Duration,
    },
    AddMana {
        who: PlayerRef,
        mana: ManaSpec,
    },
    PutCounters {
        what: EffectTarget,
        kind: CounterKind,
        n: ValueExpr,
    },
    CreateToken {
        spec: TokenSpec,
        count: ValueExpr,
        controller: PlayerRef,
    },
    /// Create a token that's a **copy** of a permanent (CR 707.9e / 111.3). The copy snapshots the
    /// `source`'s copiable characteristics (its base `chars` — name, types, colours, P/T, and its
    /// abilities via `grp_id`; **not** counters, damage, auras, or other continuous effects, CR 707.2),
    /// then applies `mods`. e.g. Applied Geometry copies a permanent "except it's a 0/0 Fractal
    /// creature" with six +1/+1 counters; Colorstorm Stallion copies itself with empty `mods`.
    CreateTokenCopy {
        source: EffectTarget,
        controller: PlayerRef,
        mods: TokenCopyMods,
    },
    /// Counter a target spell or ability (CR 701.6).
    Counter {
        what: EffectTarget,
    },
    /// Two creatures fight (CR 701.12): each deals damage equal to its power to the other,
    /// **simultaneously** (lowered to two `Damage` actions in one whiteboard so deathtouch /
    /// lethal-damage interact correctly). e.g. a fight spell = `Fight{ ChosenIndex(0), ChosenIndex(1) }`.
    Fight {
        a: EffectTarget,
        b: EffectTarget,
    },
    Search {
        who: PlayerRef,
        zone: Zone,
        filter: CardFilter,
        min: u32,
        max: u32,
        to: ZoneDest,
        /// When `to` is the battlefield, enter the found permanent(s) tapped (fetch lands like
        /// Fabled Passage / Escape Tunnel). Ignored for non-battlefield destinations.
        tapped: bool,
    },
    Tap {
        what: EffectTarget,
        tap: bool,
    },
    MoveZone {
        what: EffectTarget,
        to: ZoneDest,
    },
    Discard {
        who: PlayerRef,
        count: ValueExpr,
    },
    Exile {
        what: EffectTarget,
    },
    /// Impulse-play (SoS): exile `what` and grant its owner permission to **play** it (cast it / play
    /// it as a land) from exile until the end of `window` (CR — "you may play that card until …").
    /// Sets `Object.castable_from_exile` + `play_until_turn`; the offer loop honours the card's own
    /// timing and the expiry. e.g. Practiced Scrollsmith exiles a graveyard card castable until the end
    /// of your next turn.
    ExileForPlay {
        what: EffectTarget,
        window: PlayWindow,
    },
    /// Attach `what` onto `to` (sets `what`'s `attached_to`). `what` is usually `SourceSelf` (the
    /// Equipment equip ability, `{cost}: attach this to target creature you control`, sorcery-
    /// speed); the explicit field also covers "attach target Aura/Equipment to …" effects.
    /// Lowers to `Action::AttachTo { attachment: what, target: to }`.
    Attach {
        what: EffectTarget,
        to: EffectTarget,
    },
    /// Earthbend N (custom keyword action): the chosen land **becomes a 0/0 creature with haste
    /// that's still a land** and gets `n` +1/+1 counters (CR 611 animation + 122 counters). The
    /// "becomes a creature" part is a resolution-granted continuous effect (lowers to
    /// `Action::GrantContinuous`); the counters lower to `AddCounters`. The companion "when it
    /// dies or is exiled, return it tapped" clause is a delayed triggered ability registered at
    /// the same time (CR 603.7). `target` is "target land you control" (or a chosen land).
    Earthbend {
        target: EffectTarget,
        n: ValueExpr,
    },

    // ── composition / control flow ──────────────────────────────────────────────
    Sequence(Vec<Effect>),
    /// "You may …" — a yes/no choice gates `body` (CR 603.5 for optional triggers).
    Optional {
        prompt: String,
        body: Box<Effect>,
    },
    /// "[do `cost`]. If you do, [`reward`]" — `reward` runs only if `cost` was **actually
    /// performed**, not merely attempted. This is the engine encoding of MTG's pervasive
    /// "you may … If you do, …" template: a `cost` that can't be carried out in full (e.g. a
    /// `ForEach`/`Select` that can't reach its `min`, or a declined `Optional`) reports
    /// "not done", so the reward is withheld. Gating ties to `cost`'s real execution — never a
    /// parallel state predicate that could disagree with it. (Dyadrine: cost = "you may remove a
    /// +1/+1 counter from each of two creatures you control", reward = "draw a card and create a
    /// Robot".)
    IfYouDo {
        cost: Box<Effect>,
        reward: Box<Effect>,
    },
    /// Modal: choose `min..=max` of `modes` (CR 700.2).
    Modal {
        modes: Vec<Mode>,
        min: u32,
        max: u32,
        allow_repeat: bool,
    },
    Repeat {
        count: ValueExpr,
        body: Box<Effect>,
    },
    Conditional {
        cond: Condition,
        then: Box<Effect>,
        otherwise: Option<Box<Effect>>,
    },
    /// Apply `body` once per object in `selector` (CR uses "for each").
    ForEach {
        selector: SelectSpec,
        body: Box<Effect>,
    },
    /// Distribute `total` among the `among` set (≥ `min_each` each), running `body` per unit.
    Distribute {
        total: ValueExpr,
        among: SelectSpec,
        min_each: u32,
        body: Box<Effect>,
    },

    /// "Look at the top `count` cards of your library, put `take` of them into `take_to`, and the rest
    /// into `rest_to`" (CR 120-ish library manipulation) — the SoS Strixhaven "look-and-pick" pattern
    /// (Flow State / Stress Dream / Stirring Honormancer). The controller chooses which cards are taken;
    /// `take_to`/`rest_to` are `Hand`/`Graveyard`/`Library` (Library = the bottom, in any order).
    LookAndPick {
        count: ValueExpr,
        take: ValueExpr,
        take_to: Zone,
        rest_to: Zone,
        /// Only cards matching this filter may be **taken** (Paradox Surveyor: "reveal a land or a card
        /// with {X} in its cost"). `CardFilter::Any` = any card is takeable; a restrictive filter also
        /// makes the take *optional* (you may take none if nothing qualifies).
        take_filter: CardFilter,
    },

    /// Declare a **"target player"** (CR 115.1) for the spell/ability — a targeting slot with no
    /// effect of its own. The player-affecting effects that follow reference the chosen player via
    /// `PlayerRef::ChosenTarget(n)` (e.g. "target player draws two and loses 2 life"). Collected as a
    /// `TargetKind::Player` spec at cast (so the engine prompts for a player); at resolution it just
    /// advances the target cursor so later `Target(...)` slots line up. Use `PlayerRef::Opponent` for
    /// the forced "target opponent" case (single opponent in 2-player) rather than this.
    TargetPlayer,

    /// No-op (e.g. an unchosen optional, or a placeholder mode).
    Nothing,

    // ── escape hatch (WHITEBOARD_MODEL.md §2.3) ─────────────────────────────────
    Native {
        name: &'static str,
        run: NativeFn,
    },
}

impl Effect {
    /// Convenience: chain effects into a `Sequence`.
    pub fn seq(effects: Vec<Effect>) -> Self {
        Effect::Sequence(effects)
    }
}

/// Inline snapshot ("expect") tests pinning the shape of representative Effect IR trees. The IR
/// carries `NativeFn` pointers and so is not `serde`-serializable; we snapshot its pretty
/// `Debug` render, which reads as documentation of how a card lowers to the IR. Regenerate with
/// `UPDATE_EXPECT=1 cargo test`.
#[cfg(test)]
mod tests {
    use super::target::{TargetKind, TargetSpec};
    use super::value::{PlayerRef, ValueExpr};
    use super::{Effect, EffectTarget};
    use crate::basics::DamageKind;
    use expect_test::expect;

    /// A "Lightning Bolt"-style burn spell: deal 3 damage to any target.
    #[test]
    fn burn_spell_ir() {
        let bolt = Effect::DealDamage {
            amount: ValueExpr::Fixed(3),
            to: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Any,
                min: 1,
                max: 1,
                distinct: true,
            }),
            kind: DamageKind::Noncombat,
        };
        expect![[r#"
            DealDamage {
                amount: Fixed(
                    3,
                ),
                to: Target(
                    TargetSpec {
                        kind: Any,
                        min: 1,
                        max: 1,
                        distinct: true,
                    },
                ),
                kind: Noncombat,
            }"#]]
        .assert_eq(&format!("{bolt:#?}"));
    }

    /// A "Divination"-style draw: you draw two cards.
    #[test]
    fn draw_spell_ir() {
        let draw = Effect::Draw {
            who: PlayerRef::Controller,
            count: ValueExpr::Fixed(2),
        };
        expect![[r#"
            Draw {
                who: Controller,
                count: Fixed(
                    2,
                ),
            }"#]]
        .assert_eq(&format!("{draw:#?}"));
    }
}
