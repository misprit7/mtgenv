//! Ability kinds — the rule registry of the whiteboard model (WHITEBOARD_MODEL.md §2.3). Every
//! card ability is one of these, all *data* interpreted by the effect runtime: spell,
//! activated (incl. mana), triggered (incl. delayed/state), replacement/prevention, and
//! continuous/static (which contribute to layers + qualifications).

use super::condition::{Condition, Duration};
use super::target::{CardFilter, ManaSpec, SelectSpec};
use super::value::ValueExpr;
use super::Effect;
use crate::basics::{CardType, Color, CounterKind, DamageKind, ManaCost, Zone};
use crate::ids::{ObjId, PlayerId};
use crate::subtypes::Subtype;
use serde::{Deserialize, Serialize};

/// A cost to be paid (CR 118): an optional mana component plus any number of non-mana
/// components. `{0}`/0-life are always payable (CR 118.3a/b).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cost {
    pub mana: Option<ManaCost>,
    pub components: Vec<CostComponent>,
}

impl Cost {
    /// A "simple tap" mana-ability cost — only `{T}` (no extra mana, no sacrifice/discard/etc.). The
    /// engine's auto-payer models such a source as free-to-tap. A **cost-bearing** mana ability
    /// (`!is_simple_tap_mana()` — a Treasure's `{T}, Sacrifice this:` or Hydro-Channeler's `{1}, {T}:`)
    /// is NOT auto-payable — its extra cost must be paid through `pay_cost`, so it's offered only via
    /// manual mana activation (CR 605.3a) and kept out of the auto-pay source pool.
    pub fn is_simple_tap_mana(&self) -> bool {
        self.mana.is_none() && self.components.iter().all(|c| matches!(c, CostComponent::TapSelf))
    }
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
    /// Return permanents matching the spec (that the payer controls) to their owner's hand — Daze's
    /// alternative cost "return an Island you control to its owner's hand" (CR 118.9). `spec.min`
    /// permanents are returned; payable iff the payer controls that many matching permanents.
    ReturnToHand(SelectSpec),
    /// Remove counters from the source.
    RemoveCounters { kind: CounterKind, n: ValueExpr },
    /// Crew N (CR 702.122): tap any number of untapped creatures you control with total power ≥ N.
    Crew(u32),
    /// "Tap N untapped creatures you control" as an activation cost (Harmonized Trio's "{T}, Tap two
    /// untapped creatures you control"). A count-based sibling of [`Crew`] (which is power-based): the
    /// controller taps exactly N *other* untapped creatures they control (the source is excluded — it's
    /// already tapped by any `{T}` in the same cost). Payable iff at least N such creatures exist.
    TapCreatures(u32),
    /// An additional mana payment beyond the base cost.
    AdditionalMana(ManaCost),
    /// A planeswalker loyalty-ability cost (CR 606.2): `+N` adds N loyalty counters, `−N`
    /// removes N, `0` neither. Payable iff `n >= 0` or the source has at least `-n` loyalty
    /// counters (you can't pay a `−N` you don't have). The once-per-turn limit on loyalty
    /// abilities is **per planeswalker, across all its loyalty abilities** (606.3) — enforced
    /// by the engine, not by this cost.
    Loyalty(i32),
    /// "Exile this card from your graveyard" — both the cost (exile the source) AND the marker that
    /// this `Activated` ability is usable **from the graveyard** (CR 601.3e / graveyard-activated:
    /// Eternal Student, Stone Docent). `legal_priority_actions` scans the graveyard for abilities
    /// carrying this component; paying it moves the source card to exile.
    ExileSelfFromGraveyard,
    /// "Discard this card" — both the cost (discard the source) AND the marker that this `Activated`
    /// ability is usable **from the hand** (a cycling-style ability: Visionary's Dance). Paying it
    /// moves the source card to its owner's graveyard.
    DiscardSelfFromHand,
    /// A **pure marker** (no cost effect) that this `Activated` ability functions **from the
    /// graveyard** (CR 113.6 / 601.3e) even though its cost doesn't exile the source — e.g. a
    /// self-recursion ability "{cost}: Return this card from your graveyard to your hand/the
    /// battlefield" (Summoned Dromedary, Teacher's Pest). The effect itself moves the source out
    /// of the graveyard; this component only makes `legal_priority_actions` scan the graveyard for
    /// the ability. Always payable; paying it does nothing. (Distinct from `ExileSelfFromGraveyard`,
    /// which is both marker AND an exile cost.)
    ActivateFromGraveyard,
}

/// A spell-level **additional cost** to cast (CR 601.2b/f: "As an additional cost to cast this
/// spell, ..."). Each `AdditionalCost` is one clause the caster must pay while casting;
/// `options.len() > 1` is a **modal** clause ("... exile two cards from your graveyard OR pay
/// {1}{W}") from which the caster picks one **payable** option (CR 601.2b). A card may carry
/// several clauses (all required) via multiple [`Ability::AdditionalCost`] markers.
///
/// Paid through the real [`Cost`] machinery at CR 601.2f–h (the same point as the mana cost, with
/// any mana in an option folded into the mana payment), and required by the legality mask so a
/// spell whose additional cost can't be paid isn't offered — there is no post-payment rewind
/// (WHITEBOARD_MODEL §2.6). A [`CostComponent::PayLife`] (or other) component referencing
/// [`ValueExpr::X`] shares the spell's single chosen X (CR 107.3 — announced at cast even when the
/// printed mana cost has no `{X}`, e.g. Fix What's Broken's "pay X life").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdditionalCost {
    pub options: Vec<Cost>,
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
    /// A spell matching `filter` that **targets a creature** was cast (CR 601.2i) — the SoS
    /// "Repartee" cycle's "whenever you cast an instant or sorcery spell that targets a creature."
    /// Like [`SpellCast`], but only fires when one of the cast spell's chosen targets is a creature.
    SpellCastTargetingCreature(CardFilter),
    /// "When you cast **this** spell" (CR 702.83 Cascade, Infusion copy-self) — fires once, from the
    /// spell being cast, while it is on the stack. Unlike [`SpellCast`] (a watcher on a *battlefield*
    /// permanent observing *other* casts), this ability lives on the spell itself and is found by
    /// scanning the just-cast spell's OWN abilities (`queue_self_cast_triggers`). The trigger carries
    /// the spell as both `source` and `ctx.triggering_spell`, so its effect can read the spell's own
    /// mana value (Cascade's threshold) or copy the spell (Lumaret's Favor / Social Snub).
    SelfCast,
    /// "Whenever one or more cards leave your graveyard" (SoS Lorehold): fires once per effect
    /// resolution in which the controller's graveyard shrank.
    CardsLeaveYourGraveyard,
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
    /// One or more creatures the watcher's controller controls deal **combat damage to a player**
    /// (CR 510.1c/603.2) — fires ONCE per combat-damage step for that controller, regardless of how
    /// many creatures dealt or how much (the "one or more" batch), for each watcher whose controller
    /// dealt such damage. Distinct from per-instance `DamageDealt`. Killian's Confidence.
    YouDealCombatDamageToPlayer,
    /// **This** creature deals combat damage to a player (CR 510.1c / 603.2) — fires once per
    /// combat-damage step per creature that dealt >0 combat damage to any player (Snooping Page's
    /// "whenever this creature deals combat damage to a player, draw a card and lose 1 life"). The
    /// per-creature analogue of the batched, per-controller `YouDealCombatDamageToPlayer`.
    SelfDealsCombatDamageToPlayer,
    /// This creature blocks or becomes blocked (CR 509.1i).
    SelfBlocks,
    /// You tap a creature for mana (CR 605.1b) — drives "whenever you tap a creature for mana, add
    /// …" no-stack triggered mana abilities (Badgermole Cub). Fires per creature tapped for mana.
    TapCreatureForMana,
    /// The watcher's controller gained life (CR 603.2) — "whenever you gain life, …". Fires once per
    /// life-gain event (regardless of amount), for each permanent the gaining player controls.
    GainLife,
    /// One or more counters of `kind` were put on THIS permanent (CR 603.2) — "whenever one or more
    /// +1/+1 counters are put on this creature, …" (Pensive Professor / Berta). Fires once per
    /// counter-adding event on the source, matched on `kind`.
    CountersPutOnSelf { kind: CounterKind },
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
    /// A permanent matching `filter` **would die** — i.e. move from the battlefield to the graveyard
    /// (CR 700.4): destruction (lethal damage / destroy / toughness ≤ 0), sacrifice, or the legend
    /// rule. Patterns on the *zone move*, not just destruction, so every death path is caught. Used
    /// by "if that creature would die this turn, exile it instead" (Wilt in the Heat).
    WouldDie(CardFilter),
}

/// A serde-safe rewrite for a **floating** replacement effect stored in game state
/// ([`crate::state::FloatingReplacement`]). A subset of [`Rewrite`] excluding the `ReplaceWith(Effect)`
/// case (which can't be snapshot). The rewrite pass maps each to the corresponding [`Rewrite`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FloatingRewrite {
    /// Exile the object instead of it dying (CR 614 — the death zone-move is redirected to exile).
    ExileInstead,
    /// The object enters the battlefield with `n` extra counters of `kind` (CR 614.1e) — a
    /// resolution-created ETB rider scoped to a *specific* object, unlike the printed
    /// [`Rewrite::EntersWithCounters`] a permanent carries for its OWN ETB. Wildgrowth Archaic:
    /// "whenever you cast a creature spell, that creature enters with X additional +1/+1 counters."
    /// `n` is fixed when the rider is armed (the colours spent were determined at cast).
    EntersWithCounters { kind: CounterKind, n: u32 },
    /// "The next time the scoped SOURCE would deal damage to `protected` this turn, prevent that
    /// damage; then `deflect_source` deals that much damage to that source's controller" (CR 615 —
    /// Deflecting Palm). One-shot. The scoped object (`FloatingReplacement.scope`) is the chosen
    /// damage source; `deflect_source` is the spell dealing the redirected damage.
    PreventAndRedirectToSourceController { protected: PlayerId, deflect_source: ObjId },
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
    /// Prevent the damage a player would be dealt and deal that much to the damaging source's
    /// controller (CR 615 — Deflecting Palm). `deflect_source` is the spell that deals the redirected
    /// damage. The runtime mapping of [`FloatingRewrite::PreventAndRedirectToSourceController`].
    PreventAndRedirect { deflect_source: ObjId },
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
    /// Exile the object **instead of it dying** (CR 614 — Wilt in the Heat's "if that creature would
    /// die this turn, exile it instead"): the death action (Destroy / Sacrifice / MoveZone→graveyard)
    /// is replaced with an Exile of the same object. The `FloatingRewrite` analogue for game state.
    ExileInstead,
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
    /// Layer 6: grant a **triggered ability** (CR 613.1f — "gains [ability]"). `template_grp` names a
    /// def in the reserved template block (`grp::GRANT_TEMPLATE_BLOCK`, 9800+) carrying exactly one
    /// [`Ability::Triggered`]; the granted trigger fires from each affected object with the GRANTING
    /// card's effect (its controller = the object's controller). Referenced by grp — the same idiom as
    /// tokens/emblems/prepare-backs — so the layer/continuous state stays serde-safe (an `Ability`/
    /// `Effect` isn't). Read by the granted-ability scan in `queue_self_triggers`.
    GrantAbility { template_grp: u32 },
    /// Layer 6: grant an **activated mana ability** `{T}: Add …` to each affected object (CR 613.1f —
    /// Resonating Lute "Lands you control have '{T}: Add two mana of any one color…'"). Unlike keyword
    /// grants this can't live in [`ComputedChars`] (a mana ability isn't a keyword/type/color), so it
    /// is NOT painted there; instead the mana-payment enumeration reads it directly via
    /// [`crate::chars::granted_tap_mana`] so a granted tap-mana ability is visible to affordability and
    /// auto-pay (the `mana.produces`/`any_color` count is honoured — multi-mana-per-tap). The cost is a
    /// plain `{T}`, so it is auto-pay-usable (no cost-bearing "option-B" caveat).
    GrantTapMana { mana: ManaSpec },
    /// Layer 6: remove a keyword ability.
    RemoveKeyword(Keyword),
    /// Layer 6: grant "protection from [colour]" (CR 702.16). Painted into
    /// [`crate::chars::ComputedChars::protection_from`]; read by the targeting, damage-prevention,
    /// and can't-be-blocked-by seams. Akroma's Will mode 2 grants one per colour ("protection from
    /// each color"). (The CR 702.16 enchant/equip/attach clause is a pool-scoped omission.)
    ProtectionFromColor(Color),
    /// Layer 6: grant "hexproof from [colour]" (CR 702.11, qualified) — targeting-only, opponent-
    /// scoped. Painted into [`crate::chars::ComputedChars::hexproof_from`]. Veil of Summer grants
    /// hexproof from blue and from black to the caster's permanents.
    HexproofFromColor(Color),
    /// Layer 4: add a card type.
    AddType(CardType),
    /// Layer 4: add a subtype, keeping existing ones (CR 613.1c) — Great Hall of the Biblioplex
    /// "becomes a … Wizard" while staying a land. Timestamp-ordered with the other layer-4 effects.
    AddSubtype(Subtype),
    /// Layer 4: replace the object's **creature** subtypes (`Subtype::Creature(_)`) with exactly the
    /// given set, keeping non-creature subtypes (CR 613.1c) — Fractalize "becomes a Fractal … loses
    /// all other creature types." Later timestamp wins.
    SetCreatureSubtypes(Vec<Subtype>),
    /// Layer 5: set/add color.
    AddColor(Color),
    SetColor(Vec<Color>),
    /// A qualification marker the structural machinery respects (CR 613/§2.4).
    Qualification(Qualification),
    /// A generic cost reduction (CR 118.7) — reduces generic mana by N.
    CostReductionGeneric(u32),
    /// The controller may play N additional lands each turn (CR 305.2/505.5b) — Exploration / Azusa
    /// / Icetill Explorer. A player-level permission (read by the land-play legality, not painted
    /// on objects); `affects` scopes it to the controller.
    ExtraLandPlays(u32),
    /// The controller may play lands from `zone` (not just hand) — e.g. Crucible / Icetill's "play
    /// lands from your graveyard." A player-level permission read by the land-play legality.
    PlayLandsFrom(crate::basics::Zone),
    /// "Once during each of your turns, you may cast a `filter` spell from your hand without paying its
    /// mana cost" (Zaffai and the Tempests). A permission read by the priority-action builder, which
    /// offers a [`crate::agent::PlayableAction::CastFreeFromHand`] for each matching hand card while the
    /// granting permanent is unused this turn (`used_once_per_turn`). Not painted on objects (like
    /// `ExtraLandPlays`/`PlayLandsFrom`); the layer painter ignores it.
    FreeCastFromHandOncePerTurn { filter: CardFilter },
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
    /// Infect (CR 702.90): this creature deals combat/noncombat damage to creatures as −1/−1 counters
    /// and to players as poison counters. Read in `apply_damage`.
    Infect,
    Lifelink,
    Menace,
    Reach,
    /// Split second (CR 702.61): while a spell with split second is on the stack, players can't cast
    /// spells or activate abilities that aren't mana abilities. Read by `legal_priority_actions`.
    SplitSecond,
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
    /// This spell can't be copied (CR 707) — painted on a spell while it's on the stack (a self-static
    /// like [`CantBeCountered`]). Read at the single copy choke point [`crate::priority::EngineCore::
    /// copy_spell_on_stack`], which skips minting a copy of a spell carrying this. Choreographed Sparks.
    CantBeCopied,
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
    /// Convoke (CR 702.51): "Your creatures can help cast this spell. Each creature you tap while
    /// casting it pays for {1} or one mana of that creature's colour." A marker read by the cast path
    /// (`has_convoke`): the offer gate sizes affordability with the convoke planner, and payment taps
    /// chosen creatures to reduce the mana cost before `auto_pay`.
    Convoke,
    /// Suspend N—[cost] (CR 702.62): "Rather than cast this card from your hand, you may pay `cost`
    /// and exile it with `n` time counters on it." A time counter is removed at the beginning of the
    /// card's owner's upkeep; when the last is removed, its owner casts it without paying its mana
    /// cost (cast triggers fire — the suspended cast IS a cast, CR 702.62f). A static casting-
    /// permission ability offered by `legal_priority_actions` (as `CastVariant::Suspend`).
    Suspend { n: u32, cost: ManaCost },
    /// Flashback (CR 702.34): "You may cast this card from your graveyard for its flashback `cost`.
    /// Then exile it." A static casting-permission ability — `legal_priority_actions` scans for it to
    /// offer casting the card from the graveyard; the spell is exiled as it leaves the stack. The
    /// `cost` is a full [`Cost`] so a flashback cost can be non-mana (Group Project — "Flashback—Tap
    /// three untapped creatures you control"), paid through the real cost machinery alongside any mana.
    Flashback { cost: Cost },
    /// Kicker (CR 702.33): "You may pay an additional `cost` as you cast this spell." An **optional**
    /// additional cost — a marker read in the cast pipeline, which offers the caster (when affordable)
    /// to pay it and records [`crate::state::Object::kicked`]. A "does more if kicked" effect reads that
    /// via [`crate::effects::value::ValueExpr::IfKicked`]. Distinct from the mandatory
    /// [`Ability::AdditionalCost`].
    Kicker { cost: ManaCost },
    /// Alternative cast cost (CR 118.9): "You may [pay `cost`] rather than pay this spell's mana cost."
    /// A static casting-permission ability read at the hand-cast offer (like [`Overload`]); an
    /// alternative cast pays this non-mana [`Cost`] (Daze — return an Island; Force of Will — pay 1 life
    /// and exile a blue card) INSTEAD of the mana cost (`CastVariant::Alternative`), the spell's normal
    /// effect and targets otherwise unchanged. The `cost` may also carry mana (none in the current pool).
    AlternativeCast { cost: Cost },
    /// Overload (CR 702.96): "You may cast this spell for its overload `cost`. If you do, change 'target'
    /// in its text to 'each'." A static casting-permission ability read at the hand-cast offer; an
    /// overloaded cast pays this alternative mana cost, chooses NO targets (702.96b replaces the word),
    /// and the engine derives the "each" effect by rewriting every `Target(spec)` into a `ForEach` over
    /// the spec's matching objects (`overload_rewrite`).
    Overload { cost: ManaCost },
    /// Miracle (CR 702.94): "You may cast this card for its miracle `cost` when you draw it if it's
    /// the first card you drew this turn." A static ability that functions from the HAND: when the
    /// owner draws their first card of the turn and it's this card, a reveal trigger
    /// ([`crate::stack::StackObjectKind::MiracleWindow`]) goes on the stack; on resolution its
    /// controller may cast the card for `cost` (a fixed alternative cast cost, `CastVariant::Miracle`).
    /// Printed on some cards; also GRANTED to a filtered set by [`GrantMiracle`] (Lorehold).
    Miracle { cost: ManaCost },
    /// A static that GRANTS miracle to the controller's cards in hand matching `filter` (CR 702.94 /
    /// 118) — "Each instant and sorcery card in your hand has miracle {2}" (Lorehold, the Historian).
    /// Mirrors [`GrantCostReduction`]: the miracle-cost lookup (`miracle_cost`) checks a card's own
    /// printed [`Miracle`] AND any `GrantMiracle` on a permanent the drawer controls whose `filter`
    /// matches the drawn card, using this `cost`.
    GrantMiracle { cost: ManaCost, filter: CardFilter },
    /// Paradigm (SoS Lessons): "Then exile this spell. After you first resolve a spell with this name,
    /// you may cast a copy of it from exile without paying its mana cost at the beginning of each of
    /// your first main phases." This marker carries only the **"then exile this spell"** half — a
    /// Paradigm card (the original, not a copy) is put into **exile** rather than its graveyard as it
    /// resolves (`resolve_top` reads this, checked after the `is_copy` cease-to-exist branch). The
    /// recurring-recast half is pure data on the same card: `FunctionsFrom(vec![Zone::Exile])` +
    /// a `Triggered{ BeginningOfStep(PrecombatMain) }` whose optional effect is `CastCopy{SourceSelf}`
    /// — active only once the card reaches exile (so "after you first resolve it" falls out for free).
    /// Assemble the whole bundle with `cards::helpers::paradigm_lesson`.
    Paradigm,
    /// "Exile [this spell]" on resolution (CR 608.2n override) — an instant/sorcery that puts *itself*
    /// into exile rather than its owner's graveyard as it finishes resolving (Wisdom of Ages's "Exile
    /// Wisdom of Ages"). A pure marker read by `resolve_top` alongside the `flashback_cast`/`Paradigm`
    /// exile branch; unlike Paradigm it carries no recast half. Distinct from a mid-resolution
    /// `MoveZone{ SourceSelf → Exile }`, which the epilogue's graveyard move would override.
    ExileOnResolve,
    /// **Prepare** (SoS DFCs): a front-face creature marker linking it to its back-face spell. `spell`
    /// is the back face's registered `grp_id` (in the reserved 9700+ block). The reminder reads "While
    /// it's prepared, you may cast a copy of its spell. Doing so unprepares it." — so while a permanent
    /// carrying this marker is [`crate::state::Object::prepared`], `legal_priority_actions` offers a
    /// [`crate::agent::PlayableAction::CastPrepared`] at the back face's own timing (instant vs sorcery
    /// speed), which mints and **pays for** a copy of `spell` (CR 707.12 — a spell-copy consumer, not a
    /// CR 711 transform) and clears the prepared status. The back face is copy-only: it never enters a
    /// zone or is cast from hand, so it is registered purely as a def and excluded from the deck-builder
    /// catalog. "Becomes prepared" itself is an ordinary ability via [`crate::effects::Effect::BecomePrepared`].
    Prepare { spell: u32 },
    /// A cost-modification static (CR 601.2f / 118) applying to **casting this card**: while the
    /// card is being cast, reduce its total cost by `amount` if `condition` holds. Self-referential
    /// — read during cost determination (`effective_cast_cost`), evaluated relative to the caster.
    /// The reduction never takes the cost below {0}, and a generic-only reduction can't remove a
    /// coloured pip (CR 118.7). Multiple `CostReduction`s on one card each apply. e.g. Orysa "costs
    /// {3} less if creatures you control have total toughness 10 or greater" (a `State` condition);
    /// Ajani's Response "{3} less if it targets a tapped creature" (a `TargetMatches` condition).
    CostReduction {
        amount: CostReductionAmount,
        condition: CostReductionCondition,
        /// Whether this reduction applies to **casting this card** (`Cast`, read by
        /// `effective_cast_cost`) or to **activating this card's activated abilities** (CR 602 —
        /// `ActivatedAbilities`, read by `effective_activation_cost`, e.g. Diary of Dreams "this
        /// ability costs {1} less to activate for each page counter"). Keeps one cost-reduction
        /// mechanism serving both the spell-cast and ability-activation cost paths.
        scope: CostReductionScope,
    },
    /// A cost-modification static (CR 118) that reduces the cost of **other spells its controller
    /// casts**, scoped by a filter on the cast spell — "Instant and sorcery spells you cast have
    /// affinity for creatures" (Witherbloom, the Balancer grants affinity to your I/S spells).
    /// Distinct from [`CostReduction`], which is self-referential (reduces THIS card's own cost):
    /// this lives on a *granting* permanent and reduces a *different* card's cost. Gathered by
    /// `effective_cast_cost` from every permanent the caster controls; each grant whose `spell_filter`
    /// matches the cast card applies its `amount` (evaluated relative to the caster/granting permanent,
    /// so `GenericValue(Count{ creatures you control })` = affinity for creatures). Generic-only, like
    /// affinity (CR 702.40) — never removes a coloured pip.
    GrantCostReduction {
        amount: CostReductionAmount,
        spell_filter: CardFilter,
    },
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
    /// A spell-level **additional cast cost** (CR 601.2b/f) — "As an additional cost to cast this
    /// spell, …". A marker read at cast time (like [`CostReduction`], it lives in the card's
    /// ability list rather than adding a `CardDef` field, so no `CardDef` literal needs touching).
    /// Multiple markers = multiple required clauses. See [`AdditionalCost`].
    AdditionalCost(AdditionalCost),
    /// A marker that this card's **triggered abilities function from the listed zone(s)** in
    /// addition to the battlefield default (CR 113.6 — "an ability functions only while its source
    /// is on the battlefield unless the ability states otherwise"). The default zone-of-function is
    /// implicit, so only *deviating* cards carry this marker — zero churn on the pool. Killian's
    /// Confidence carries `FunctionsFrom(vec![Zone::Graveyard])` so its combat-damage trigger fires
    /// while it sits in the graveyard. `collect_triggers` scans the marked zones (graveyard today;
    /// the scan generalizes to hand/exile — madness/suspend-style — as those markers arrive).
    FunctionsFrom(Vec<Zone>),
}

/// How much an [`Ability::CostReduction`] takes off a spell's total cost (CR 118).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostReductionAmount {
    /// Reduce generic mana by a fixed `n` (floored at 0) — "this spell costs {N} less to cast".
    Generic(u32),
    /// Reduce generic mana by the value evaluated at cost determination — "{1} less for each …"
    /// (The Dawning Archaic: `GenericValue(Count{ I/S cards in your graveyard })`).
    GenericValue(ValueExpr),
    /// Reduce by a coloured/generic cost (CR 118.6) — removes matching coloured pips too, then
    /// generic. e.g. Brush Off's "{1}{U} less". (Deferred consumer; the leaf is here for generality.)
    Cost(ManaCost),
}

/// Which cost an [`Ability::CostReduction`] modifies — casting this card, or activating this card's
/// activated abilities (CR 601.2f vs 602). One mechanism, two application paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostReductionScope {
    /// Reduces the cost to **cast this card** (`effective_cast_cost`). Orysa, Ajani's Response.
    Cast,
    /// Reduces the cost to **activate this card's activated abilities** (`effective_activation_cost`).
    /// Applies to all of the source's `Activated` abilities (no card yet distinguishes per-ability).
    /// Diary of Dreams.
    ActivatedAbilities,
}

/// When an [`Ability::CostReduction`] applies (CR 601.2f). Two flavours — a caster-relative
/// **state/count** condition, and a **target-dependent** one that reads the spell's chosen targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostReductionCondition {
    /// The reduction applies iff `Condition` holds relative to the caster — a property of the
    /// game state, independent of the spell's targets (e.g. Orysa's "total toughness ≥ 10", Wilt
    /// in the Heat's "a card left your graveyard this turn"). Evaluated identically at the offer
    /// gate and at cast, so affordability and payment always agree.
    State(Condition),
    /// The reduction applies iff **the spell targets** an object matching this filter (CR 601.2f
    /// "if it targets a …"). Because the discount depends on the chosen targets — not known until
    /// 601.2c — cost is finalized *after* targets are chosen, and the offer gate applies the
    /// reduction optimistically (iff a legal matching target exists). e.g. Ajani's Response "if it
    /// targets a tapped creature", Run Behind "an attacking creature". With multiple targets the
    /// reduction applies if **any** chosen target matches.
    TargetMatches(CardFilter),
}
