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

use self::ability::{Cost, Keyword};
use self::condition::{Condition, Duration};
use self::native::NativeFn;
use self::target::{
    CardFilter, ManaSpec, PlayerFilter, SelectSpec, TargetSpec, TokenCopyMods, TokenSpec,
};
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
    /// The **top card of a player's library** (CR 401.1, the last element of the library vec). Not a
    /// chosen target — resolved at resolution time to whatever is on top. Used by impulse-play from
    /// the top of library (SoS: Elemental Mascot's "exile the top card of your library"). Resolves to
    /// `None` (no-op) if that library is empty.
    TopOfLibrary(PlayerRef),
    /// The **spell or ability that triggered this ability** (CR 603.2) — the object on the stack
    /// whose targeting fired a `BecomesTargeted` trigger. Not a chosen target; resolved from the
    /// resolution context (`triggering_stack`). Used by Ward (CR 702.21) to counter "that spell or
    /// ability." Resolves to `None` if the triggering object has already left the stack.
    Triggering,
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
    /// Grant a **triggered ability** to the target for a duration (CR 613.1f) — "gains 'When this
    /// creature dies, draw a card' until end of turn" (Rabid Attack) / "'Whenever this creature
    /// attacks, you gain 1 life'" (Root Manipulation). `template_grp` names a one-ability def in the
    /// reserved `grp::GRANT_TEMPLATE_BLOCK` (9800+). Lowers to `GrantContinuous{ GrantAbility{
    /// template_grp }, duration }` (same path as `GrantKeyword`); the granted trigger fires from the
    /// affected object with the granting card's effect and expires with the continuous effect.
    GrantAbility {
        what: EffectTarget,
        template_grp: u32,
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
    /// The target's **base** power/toughness become the given values for a duration (CR 613 layer
    /// 7b) — "target creature has base power and toughness 5/5 until end of turn" (Quandrix Charm).
    /// Lowers to `GrantContinuous{ SetBasePT, duration }`; later +N/+N (layer 7c) still stacks on top.
    SetBasePT {
        what: EffectTarget,
        power: i32,
        toughness: i32,
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
        /// Counters whose count is computed at **resolution** and put on each created token (e.g.
        /// "put X +1/+1 counters on it" — the Quandrix Fractal pattern). Each `(kind, n)` is
        /// evaluated ctx-aware and merged onto the token's `spec.counters` when it's created. Empty
        /// for fixed-counter / no-counter tokens (which use `spec.counters` directly).
        dynamic_counters: Vec<(CounterKind, ValueExpr)>,
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
    /// "Cast a copy of `source` without paying its mana cost" (CR 707.12). Distinct from
    /// `CreateTokenCopy` (707.9e — a token on the battlefield) and from copying a spell already on
    /// the stack (707.10 — a copy that *isn't cast*): this **casts** the copy, so it goes through the
    /// normal cast pipeline (601.2a–h) — new modes, new targets (707.10c), X=0 — and cast-triggers
    /// fire. The copy is a fresh object built from `source`'s copiable characteristics (CR 707.2 — its
    /// base `chars`, so abilities/effect/mana cost ride along via `grp_id`), marked `Object.is_copy`
    /// so it ceases to exist when it leaves the stack (707.10a). `source` is usually `SourceSelf`
    /// (Paradigm's exiled Lesson casting a copy of itself); a `Target` form covers "cast a copy of
    /// target …". `controller` is the player who casts (and thus owns, CR 707.10) the copy.
    CastCopy {
        source: EffectTarget,
        controller: PlayerRef,
    },
    /// "Cast `what` without paying its mana cost" — casts the **actual card** (not a copy), moving it
    /// from wherever it is onto the stack and running the real cast pipeline for {0} (CR 601.2f).
    /// Unlike [`CastCopy`] (which mints a copy that ceases to exist), this is the card itself — a
    /// granted flashback-style recast (The Dawning Archaic: "cast target instant or sorcery card from
    /// your graveyard without paying its mana cost"). When `exile_on_leave` is set, the freshly-cast
    /// card is flagged to be **exiled as it leaves the stack** instead of going to the graveyard
    /// (reusing the flashback exile-on-leave-stack path) — the Archaic's "if that spell would be put
    /// into your graveyard, exile it instead" rider. `what` is typically an **up-to-one** target
    /// (`min: 0`) so declining to cast = choosing no target.
    CastForFree {
        what: EffectTarget,
        exile_on_leave: bool,
    },
    /// "Exile cards from the top of `who`'s library until you exile cards with total mana value
    /// `total_mana_value` or greater. You may cast any number of them without paying their mana costs"
    /// (Improvisation Capstone) — the resolution-time free-cast-from-exile (CR 601.3e / 118.9). Exiles
    /// one card at a time until the exiled cards' running total mana value reaches the threshold (or the
    /// library empties), then loops offering the controller to cast any number of the exiled **nonland**
    /// cards (spells) for free — each a real `cast_spell(WithoutPayingManaCost)` (own targets/X, onto the
    /// stack). Uncast cards stay exiled. Interactive, so it lives in `interpret`.
    ExileTopUntilManaValueMayCastFree {
        who: PlayerRef,
        total_mana_value: u32,
    },
    /// "Mill `count` cards. Then put a creature card from among them onto the battlefield" (Bind to
    /// Life). `who` mills from their own library (so the put creature is theirs — owner == controller,
    /// no control override); the just-milled cards are the eligible set ("from among them"), and the
    /// controller chooses which creature to put (mandatory if any creature was milled — CR "put a … card").
    /// Imperative (mills + asks + moves), so it lives in `interpret`.
    MillThenPutCreatureOntoBattlefield {
        who: PlayerRef,
        count: ValueExpr,
    },
    /// "Put target card onto the battlefield **under your control**" (Reanimate) — the control-override
    /// reanimation (CR 111.1a / 400.7). Distinct from [`Effect::MoveZone`]→Battlefield, which always
    /// enters under the card's *owner*'s control: this enters under the effect's **controller** even
    /// when the card is owned by another player (a graveyard-steal). `what` is a chosen target (a
    /// graveyard card, collected by `collect_specs_into`); at resolution the object is put onto the
    /// controller's battlefield via `move_object(_, Battlefield, controller)` — `owner` is unchanged
    /// (so it returns to its owner's graveyard on death), and enters-the-battlefield triggers fire.
    /// Imperative (a control-carrying zone move), so it lives in `interpret`.
    ReanimateUnderControl {
        what: EffectTarget,
    },
    /// "Exile `what`, then return that card to the battlefield under its owner's control" (CR 603.6e
    /// blink/flicker) — All Aboard. Exiles the target then immediately returns it as a **new** object:
    /// enters-the-battlefield triggers fire, and counters / marked damage / auras / summoning-sickness
    /// all reset (CR 400.7). `what` is chosen at cast (collected by `collect_specs_into`). Imperative
    /// (two zone moves + broadcasts), so it lives in `interpret`.
    Blink {
        what: EffectTarget,
    },
    /// **Timed blink** — exile `what` now, then return that card to the battlefield under its owner's
    /// control at the beginning of the next end step (CR 603.7 delayed trigger). The SoS "Repartee"
    /// cycle (Conciliator's Duelist) and Ennis's ETB. Distinct from [`Blink`] (which returns during the
    /// same resolution): the return is a delayed [`crate::effects::action::DelayedTriggerEvent::
    /// AtBeginningOfNextEndStep`] carrying a `MoveZone{ →Battlefield }`. Imperative (exile + arm the
    /// delayed trigger), so it lives in `interpret`. A token exiled this way simply ceases to exist.
    ExileReturnNextEndStep {
        what: EffectTarget,
    },
    /// "When you next cast a `filter` spell this turn, copy that spell" (CR 707.10) — Striking Palette
    /// ("… an instant or sorcery spell this turn, copy that spell. You may choose new targets for the
    /// copy."). Arms a one-shot delayed triggered ability (CR 603.7,
    /// [`crate::effects::action::DelayedTriggerEvent::YouCastSpell`]) on the controller; when they next
    /// cast a matching spell it fires a [`crate::stack::StackObjectKind::SpellCopyTrigger`] over that
    /// spell, which on resolution mints an `is_copy` copy on the stack (NOT cast — no cast triggers,
    /// distinct from [`CastCopy`]'s 707.12 mint+cast). `choose_new_targets` offers the 707.10c "you may
    /// choose new targets" reselection. Expires unfired at the next turn's start.
    CopyNextSpellCast {
        filter: CardFilter,
        choose_new_targets: bool,
    },
    /// "Copy a spell that's on the stack `count` times" (CR 707.10) — the storm / casualty / infusion
    /// engine over the built [`crate::priority::EngineCore::copy_spell_on_stack`]. `what` names which
    /// stack spell to copy: [`EffectTarget::Triggering`] = the spell that fired this "whenever you cast
    /// …" ability (read from `ctx.triggering_spell`), the Storm/Casualty/Infusion case; a
    /// `Target`/`Select` naming a spell on the stack covers "copy target instant or sorcery spell"
    /// (Choreographed Sparks). Each of the `count` copies is minted **over** the original (so they
    /// resolve first), is **not cast** (707.10a — no `SpellCast`, no cast triggers), carries the
    /// original's chosen targets/X/modes (707.10b), and ceases to exist when it leaves the stack.
    /// `choose_new_targets` offers the 707.10c "you may choose new targets" reselection **per copy**.
    /// `count` ≤ 0 (or the source spell already gone) → no copies. Imperative (mints objects, may ask
    /// for new targets), so it lives in `interpret`. Distinct from [`CopyNextSpellCast`] (which arms a
    /// delayed trigger to copy the *next* cast) and [`CastCopy`] (707.12 — mints **and casts** a copy).
    CopySpellOnStack {
        what: EffectTarget,
        count: ValueExpr,
        choose_new_targets: bool,
    },
    /// "This creature becomes prepared" (SoS "Prepare" DFCs). Sets the `prepared` status on the
    /// ability's source (`ctx.source`) — every "becomes prepared" clause (enters-prepared,
    /// at-the-beginning-of-your-first-main, whenever-this-attacks, landfall, an activated ability, …)
    /// is just an ordinary trigger/ability whose effect is this leaf, so no new trigger machinery is
    /// needed. Lowers to [`crate::effects::action::Action::SetPrepared`]. While the source is prepared
    /// its controller may cast a *paid copy* of its back-face spell (an
    /// [`crate::agent::PlayableAction::CastPrepared`]), which unprepares it — the spell-copy subsystem
    /// (CR 707) is the substrate; the back face is copy-only and never cast from hand.
    BecomePrepared,
    /// "You get an emblem with '…'" (CR 114). Puts an emblem — an object with no characteristics
    /// other than the abilities of the registered def `emblem` (in the reserved 9000+ block) — into
    /// the controller's command zone. The emblem's ability functions from `Zone::Command` (its def
    /// carries `Ability::FunctionsFrom(vec![Zone::Command])`), so its triggers fire from there.
    /// Emblems are permanent and untouchable by removal/SBAs. (Planeswalker ultimates, CR 606/114.)
    CreateEmblem {
        emblem: u32,
    },
    /// "If [`what`] would die this turn, exile it instead" (CR 614) — registers a one-shot floating
    /// replacement ([`crate::state::FloatingReplacement`]) scoped to the object for the rest of the
    /// turn. Wilt in the Heat's rider on the creature it damages. General: the death (battlefield→
    /// graveyard) zone-move is redirected to exile, catching destruction, sacrifice, and legend-rule
    /// deaths alike.
    ExileIfWouldDie {
        what: EffectTarget,
    },
    /// Counter a target spell or ability (CR 701.6).
    Counter {
        what: EffectTarget,
    },
    /// "Counter `what` unless that spell/ability's controller pays `cost`" (CR 701.5 / 702.21) —
    /// the Ward soft-counter. `what` is typically `EffectTarget::Triggering` (the spell or ability
    /// that targeted the Ward permanent). The *targeting player* (not the Ward controller) decides
    /// whether to pay; if they can't afford it or decline, the object is countered. Reuses the
    /// engine's `Cost` (mana / pay-life / discard), so Ward {N} / Ward—Pay life / Ward—Discard all
    /// share one leaf.
    CounterUnlessPay {
        what: EffectTarget,
        cost: Cost,
    },
    /// "You may tap or untap `what`" (CR 701 — Rejoinder). The target is chosen at cast (so `what` is
    /// a `Target`, collected by `collect_specs_into`); at resolution its controller may decline, or
    /// choose to **tap** or **untap** it. Interactive (two `Confirm`s: opt-in, then direction), so it
    /// lives in `interpret`. Distinct from the fixed-direction [`Effect::Tap`].
    MayTapOrUntap {
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
        /// When `to` is the battlefield, the permanent enters **tapped** (CR 110.5 — e.g. Teacher's
        /// Pest's "return this card from your graveyard to the battlefield tapped"). Ignored for
        /// non-battlefield destinations. Mirrors `Effect::Search { tapped }`.
        tapped: bool,
    },
    Discard {
        who: PlayerRef,
        count: ValueExpr,
    },
    /// "Target player reveals their hand. You choose N `filter` card(s) from it. That player
    /// discards them" (CR 701.8 discard driven by *another* player's choice — Render Speechless,
    /// Coercion, Thoughtseize-likes). Unlike `Discard` (the discarding player chooses which),
    /// `chooser` (usually the caster) picks, then `who` discards the picks. Mandatory up to the
    /// number of eligible cards; if fewer than `count` match `filter`, all eligible are chosen.
    DirectedDiscard {
        /// The player whose hand is revealed and who discards — a `PlayerRef::ChosenTarget(n)`
        /// bound by a preceding `TargetPlayer` slot (or a relative ref).
        who: PlayerRef,
        /// The player who chooses which card(s) — usually `PlayerRef::Controller` ("you choose").
        chooser: PlayerRef,
        count: ValueExpr,
        /// Which cards are eligible to be chosen (e.g. `Not(HasCardType(Land))` = "a nonland card").
        filter: CardFilter,
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
    /// "You may pay `cost`. If you do, [`then`]." (CR — the mana/cost analogue of [`IfYouDo`], whose
    /// `cost` is an *effect*; this one pays a real [`Cost`], e.g. `{W/B}` mana.) The resolving
    /// ability's controller is asked to pay iff the cost is payable; on payment `then` runs. Killian's
    /// Confidence ("you may pay {W/B}; if you do, return this from your graveyard to your hand"). A
    /// broadly-reusable leaf — "you may pay …, if you do …" is everywhere in modern sets.
    MayPayCost {
        cost: Cost,
        then: Box<Effect>,
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
    /// Apply `body` once per **chosen target** in a variable multi-target `slot` — "tap up to two
    /// target creatures. Put a stun counter on each of them" (Homesickness). The slot is declared as
    /// a targeting spec at cast (CR 601.2c) exactly like any `Target(...)`; at resolution each chosen
    /// target is bound to `EffectTarget::Each` in turn while `body` runs. `body` MUST reference its
    /// per-iteration target via `Each` (not `Target`/`ChosenIndex`); the loop itself consumes the
    /// slot's cursor positions. Distinct from `ForEach`, which iterates a resolution-time `Select`
    /// rather than the spell's locked targets.
    ForEachTarget {
        slot: TargetSpec,
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
    /// `TargetKind::Player(filter)` spec at cast (so the engine prompts for a legal player); at
    /// resolution it just advances the target cursor so later `Target(...)` slots line up. The
    /// `PlayerFilter` restricts the candidates ("target opponent" = `PlayerFilter::Opponent`).
    TargetPlayer(PlayerFilter),

    /// "Target [creature]'s owner puts it on their choice of the top or bottom of their library"
    /// (Run Behind). The **owner** of `what` (not the caster) chooses top vs bottom, then the object
    /// moves to their library. Interactive (asks the owner), so it lives in `interpret`.
    PutOnTopOrBottom {
        what: EffectTarget,
    },

    /// "Put `count` cards from your hand on top of your library in any order" (Brainstorm's second
    /// half). The controller selects `count` cards from their hand (mandatory, up to hand size) and
    /// chooses their order; the cards move onto the **top** of the library with the first selected
    /// ending up on top (drawn first). Interactive (a resolution-time hand selection), so it lives in
    /// `interpret`. Composes with a preceding `Draw` (Brainstorm = `Sequence[Draw 3, PutFromHandOnTop 2]`).
    PutFromHandOnTop {
        who: PlayerRef,
        count: ValueExpr,
    },

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
