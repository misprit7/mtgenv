//! The single decision boundary: the [`Agent`] trait + [`DecisionRequest`] /
//! [`DecisionResponse`] + [`PlayerView`] (info-filtered). Every player choice — scripted AI,
//! Python RL, web/GRE client — flows through here; the engine pre-enumerates the *legal*
//! options (masking is the engine's job).
//!
//! Spec & rationale: `docs/design/AGENT_INTERFACE.md`. The `DecisionRequest` set is a strict
//! superset of MTGA's GRE `*Req` catalog (validated against `../mtga-re/`); its granularity is
//! driven by that catalog + the Comprehensive Rules + the engine's own decision points. All
//! types derive `serde` so a non-in-process backend (the GRE-protocol server fronting the web
//! client / real MTGA client, or any other socket agent) sits behind the trait via a thin,
//! lossless, table-driven adapter (§1.1).
//!
//! Layering: this module depends only on `ids` + `basics` — NOT on the Effect IR (`effects`).
//! The boundary is expressible independent of card data; the engine resolves effect choice
//! points into these requests.

use crate::basics::{Color, CounterKind, CounterBag, ManaCost, ManaPool, Phase, Status, Target, Zone, ZoneDest};
use crate::ids::{ObjId, PlayerId, StackId};
use serde::{Deserialize, Serialize};

// ════════════════════════════════════════════════════════════════════════════════════════
// The trait
// ════════════════════════════════════════════════════════════════════════════════════════

/// One per seat. The engine calls [`decide`](Agent::decide) whenever that seat must choose.
/// This is the entire decision surface of the engine — nothing else asks a player anything.
pub trait Agent {
    /// The engine has reached a choice point for `view.seat`. `req` enumerates the *complete
    /// legal option set* plus constraints; return a response selecting among those options.
    /// The engine validates the response is in range (§4 contract); a correct agent never
    /// produces an out-of-range one.
    fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse;

    /// Push-only notification: a public reveal, a die-roll result, an opponent's chosen value,
    /// a zone change — anything the seat should learn but need not answer. Default no-op.
    /// Maps to the GRE `GameStateMessage` diff stream.
    fn observe(&mut self, _view: &PlayerView, _ev: &GameEvent) {}
}

// ════════════════════════════════════════════════════════════════════════════════════════
// PlayerView — the information-filtered state (§2)
// ════════════════════════════════════════════════════════════════════════════════════════

/// The agent's entire window onto the game, computed from full `GameState` by the masking
/// function `view_for(seat)` (the one correct place to enforce hidden information). Hidden zones
/// are masked, opponent hand is a count, library order is hidden, face-down objects collapse to
/// a stub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerView {
    pub seat: PlayerId,
    pub turn: u32,
    pub active_player: PlayerId,
    pub phase: Phase,
    pub priority_player: Option<PlayerId>,
    /// Public facts about every seat (including self).
    pub players: Vec<PlayerPublicView>,
    /// Self-only private detail (full hand, known library tops).
    pub me: PlayerPrivateView,
    /// All permanents on the battlefield (public), as this seat may perceive them.
    pub battlefield: Vec<ObjView>,
    /// Spells + abilities on the stack.
    pub stack: Vec<StackObjView>,
    /// Combat state, when in a combat phase.
    pub combat: Option<CombatView>,
    /// This seat's own priority-stop settings, for display (which steps auto-pass vs. stop
    /// under the Arena profile). `None` when no auto-pass profile is active. Self-only render
    /// data — see [`StopStateView`].
    pub stops: Option<StopStateView>,
}

/// Public facts about one seat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerPublicView {
    pub player: PlayerId,
    pub life: i32,
    pub poison: u32,
    pub hand_count: u32,
    pub library_count: u32,
    pub graveyard: Vec<ObjView>,
    pub exile_public: Vec<ObjView>,
    pub mana_pool: ManaPool,
    pub counters: CounterBag,
}

/// Self-only private detail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerPrivateView {
    pub hand: Vec<ObjView>,
    /// Tops of the library this seat has been shown (scry/Sylvan Library), top-first.
    pub known_library: Vec<ObjView>,
    /// Cards an effect has revealed only to this seat.
    pub revealed_to_me: Vec<ObjView>,
}

/// A single object as this seat is allowed to perceive it. Hidden/face-down objects collapse to
/// a `Hidden` stub (id + zone + controller only).
// `Visible` is intentionally the large, common variant (full perceived characteristics); the
// view is built fresh each decision, not stored long-term, so the size gap is not worth boxing.
#[allow(clippy::large_enum_variant)]
// `PartialEq` lets the replay delta-encoder detect unchanged zones frame-to-frame (replay.rs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ObjView {
    Visible {
        id: ObjId,
        chars: CharacteristicsView,
        controller: PlayerId,
        owner: PlayerId,
        zone: Zone,
        status: Status,
        counters: CounterBag,
        damage_marked: u32,
        attachments: Vec<ObjId>,
        /// Summoning sickness (can't attack / use {T}) — CR 302.6.
        summoning_sick: bool,
    },
    Hidden {
        id: ObjId,
        zone: Zone,
        controller: PlayerId,
    },
}

/// The post-layer computed characteristics an agent sees (the CR 613 output projected for the
/// view). Distinct from the engine-internal characteristics cache (`chars`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CharacteristicsView {
    pub name: String,
    pub card_types: Vec<String>,
    pub subtypes: Vec<String>,
    pub supertypes: Vec<String>,
    pub colors: Vec<Color>,
    pub mana_value: u32,
    /// The printed mana cost (structured generic + colored pips). `mana_value` is the derived
    /// scalar; this is what a card-frame UI renders exact pips from (e.g. `{1}{G}` vs `{G}{G}`).
    /// `None` for objects with no mana cost (lands, abilities, tokens). (Hybrid/Phyrexian costs
    /// will need a richer `ManaCost` once the engine's cost model supports them — M5+.)
    #[serde(default)]
    pub mana_cost: Option<ManaCost>,
    /// The card's warp cost (CR 702.x `Ability::Warp`), if any — so a UI can label the warp cast
    /// option with its actual cost ("Warp {2}{G}") rather than the normal mana cost. `None` otherwise.
    #[serde(default)]
    pub warp_cost: Option<ManaCost>,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub keywords: Vec<String>,
    /// Printed oracle/rules text, for display. Empty for vanilla cards (and until the card-data
    /// layer carries text). Populated from the card definition by the view masking function.
    #[serde(default)]
    pub rules_text: String,
    /// Oracle id / printing id for embedding-table lookups (RL) and rendering.
    pub grp_id: u32,
    /// Engine-fidelity flag from the card definition (CR-agnostic): `Some(true)` = every clause is
    /// faithfully implemented, `Some(false)` = a clause is deferred (documented in `rules_text`),
    /// `None` = no card data (engine-generated objects: abilities, tokens). The client renders a ⚠
    /// marker iff this is `Some(false)`; `None`/`Some(true)` show nothing.
    #[serde(default)]
    pub fully_implemented: Option<bool>,
}

/// A spell or ability on the stack, as perceived.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackObjView {
    pub id: StackId,
    pub controller: PlayerId,
    pub source: Option<ObjId>,
    pub chars: CharacteristicsView,
    pub targets: Vec<Target>,
}

/// Combat state as perceived (CR 506–511).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatView {
    /// (attacker, what it's attacking).
    pub attackers: Vec<(ObjId, Target)>,
    /// (blocker, the attacker it's blocking).
    pub blockers: Vec<(ObjId, ObjId)>,
}

/// A seat's priority-stop settings, surfaced for display (the Arena auto-pass profile, §8.1).
///
/// **This is settings ECHO, not game state** (interim). In the GRE these settings are a distinct
/// sub-protocol (the client *sets* them via a settings message; the server *enforces and echoes*
/// them) — not part of the game-state view. We model only the echo half here, folded into
/// `PlayerView` as self-only render data; the *set* half (a client changing its own stops through
/// the protocol) is engine-side config today and would, if exposed, become a settings exchange.
/// `per_step` lists the effective stop at each priority-granting step for this seat right now.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StopStateView {
    pub full_control: bool,
    pub per_step: Vec<(Phase, bool)>,
}

// ════════════════════════════════════════════════════════════════════════════════════════
// DecisionRequest — the enumerated, masked choice set (§3)
// ════════════════════════════════════════════════════════════════════════════════════════

/// The closed set of every choice the engine can ask. Each variant pre-enumerates the legal
/// options. See `docs/design/AGENT_INTERFACE.md` §3/§6.1 for the GRE `*Req` each variant
/// subsumes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionRequest {
    /// Pick who takes the first turn. GRE: `ChooseStartingPlayerReq`.
    ChooseStartingPlayer { candidates: Vec<PlayerId> },

    /// London mulligan keep-or-mulligan. On keep after N mulligans the engine then issues a
    /// `SelectCards{ reason: BottomForMulligan }`. GRE: `MulliganReq` → `MulliganResp`.
    Mulligan {
        hand: Vec<ObjId>,
        mulligans_taken: u32,
        will_bottom_if_kept: u32,
    },

    /// Priority: cast/activate/play-land/special-action/pass. GRE: `ActionsAvailableReq`.
    Priority {
        actions: Vec<PlayableAction>,
        can_pass: bool,
    },

    /// Choose modes for a modal spell/ability (CR 700.2). GRE: part of `CastingTimeOptionsReq`.
    ChooseModes {
        for_action: ActionRef,
        modes: Vec<ModeOption>,
        min: u32,
        max: u32,
        allow_repeat: bool,
    },

    /// Choose a number (X, cost reduction, …). Legal set = `[min,max]` by `step`, minus
    /// `forbidden`, minus excluded parity. GRE: `NumericInputReq` → `NumericInputResp`.
    ChooseNumber {
        reason: NumberReason,
        min: i64,
        max: i64,
        step: u32,
        forbidden: Vec<i64>,
        disallow_even: bool,
        disallow_odd: bool,
    },

    /// The cast-time *optional costs* a caster may opt into at CR 601.2b (kicker, buyback,
    /// bargain, the decision to pay casualty, …). Answered by `Indices` (which costs are paid).
    /// VALUE-bearing cast choices — X, modal mode selection, mana-type — are NOT bundled here:
    /// the engine issues each as its own `ChooseNumber` / `ChooseModes` / `ChooseColor` decision
    /// so every request has a clean, flat, unambiguous response. (A real-MTGA-client GRE adapter
    /// reassembles these + the separate value decisions into one `CastingTimeOptionsReq`, whose
    /// inner `oneof` mirrors them — that aggregation is the adapter's job, not the boundary's.)
    /// GRE: CastingTimeOptionsReq (the optional/additional-cost slots).
    CastingTimeOptions {
        for_action: ActionRef,
        options: Vec<CastOption>,
    },

    /// Choose targets for one action — one `TargetSlot` per "target" instance, each with its own
    /// pre-filtered legal candidate list. GRE: `SelectTargetsReq` → `SubmitTargetsResp`.
    ChooseTargets {
        for_action: ActionRef,
        /// The object whose ability/spell is choosing targets — the cast spell, the activated
        /// ability's permanent, or the triggering/reflexive permanent. `None` only when there's no
        /// associated object. Lets a view tie a target decision to its source (e.g. the obs encoder
        /// recovering the source `grp_id`) without the object being on the stack yet — triggers
        /// choose targets *before* they're pushed (CR 603.3d).
        source: Option<ObjId>,
        slots: Vec<TargetSlot>,
    },

    /// Divide/distribute an amount among recipients (≥ `min_each` each). GRE: `DistributionReq`.
    Distribute {
        reason: DistributeReason,
        among: Vec<Target>,
        total: u32,
        min_each: u32,
        max_each: Option<u32>,
    },

    /// Pay a cost: which mana to spend, which permanents to tap/sac/exile, life, etc. GRE:
    /// `PayCostsReq`.
    PayCost {
        cost: CostRequest,
        mana_sources: Vec<ManaSource>,
        non_mana: Vec<PaymentOption>,
    },

    /// Declare attackers. GRE: `DeclareAttackersReq` → `SubmitAttackersResp`.
    DeclareAttackers { eligible: Vec<AttackerOption> },

    /// Declare blockers. GRE: `DeclareBlockersReq` → `SubmitBlockersResp`.
    DeclareBlockers {
        eligible: Vec<BlockerOption>,
        attackers: Vec<ObjId>,
    },

    /// Assign combat damage from one source among its recipients (lethal threshold supplied).
    /// GRE: `AssignDamageReq`.
    AssignCombatDamage {
        source: ObjId,
        recipients: Vec<DamageSlot>,
        total: u32,
        deathtouch: bool,
        trample_to: Option<Target>,
    },

    /// Order a set of objects (triggers/blockers/cards-to-zone/cost sequence). GRE: `OrderReq`.
    OrderObjects { kind: OrderKind, items: Vec<ObjId> },

    /// Choose `min..=max` objects from a pre-filtered set for some effect. GRE: `SelectNReq` /
    /// `SearchReq` / `RevealHandReq`.
    SelectCards {
        reason: SelectReason,
        from: Vec<ObjId>,
        min: u32,
        max: u32,
        /// Human/UI description of the selection constraint (the legal set is already `from`).
        description: String,
    },

    /// Choose from multiple distinct groups at once. GRE: `SelectNGroupReq`/`SelectFromGroupsReq`.
    SelectFromGroups {
        reason: SelectReason,
        groups: Vec<SelectGroup>,
    },

    /// Stage the top N cards of a library into ordered destinations (scry/surveil/fateseal).
    /// GRE: composed from `SelectNReq` + `OrderReq`.
    ArrangeCards {
        reason: ArrangeReason,
        cards: Vec<ObjId>,
        destinations: Vec<ZoneDest>,
    },

    /// Choose which replacement/prevention effect applies to an event (CR 616.1f). GRE:
    /// `SelectReplacementReq`.
    ChooseReplacement {
        event: String,
        applicable: Vec<ReplacementOption>,
    },

    /// Choose a counter kind (proliferate, "remove a counter", …). GRE: `SelectCountersReq`.
    ChooseCounterType { options: Vec<CounterKind> },

    /// Choose one/several from a labeled option list (type/vote/keyword/face/name). GRE:
    /// `SelectNReq` over a `StaticList`, or `StringInputReq` for name input.
    ChooseOption {
        reason: OptionReason,
        options: Vec<OptionLabel>,
        min: u32,
        max: u32,
    },

    /// Choose color(s). GRE: `SelectNReq` over a color `StaticList` / `SelectManaTypeReq`.
    ChooseColor {
        allowed: Vec<Color>,
        min: u32,
        max: u32,
    },

    /// A yes/no (or this-or-that) binary. GRE: `OptionalActionMessage` → `OptionalResp`.
    Confirm { kind: ConfirmKind },
}

// ════════════════════════════════════════════════════════════════════════════════════════
// DecisionResponse (§4)
// ════════════════════════════════════════════════════════════════════════════════════════

/// Selections into the request's option vectors (+ payloads for the few structured variants).
/// Because the engine enumerated only legal options, any in-range selection is legal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DecisionResponse {
    /// Pass priority (only valid when `Priority { can_pass: true }`).
    Pass,
    /// Pick exactly one option by index.
    Index(u32),
    /// Pick a subset by indices (modes, cards, attackers, colors, multi-target).
    Indices(Vec<u32>),
    /// A number (X, cost reduction, generic "choose a number").
    Number(i64),
    /// Yes/no for `Confirm`.
    Bool(bool),
    /// Pairs `(selector_idx, target_idx)` — blocker→attacker, target→slot maps.
    Pairs(Vec<(u32, u32)>),
    /// Distribution `(recipient_idx, amount)` — sums to `total`, each ≥ `min_each`.
    Amounts(Vec<(u32, u32)>),
    /// A permutation of the request's `items` indices (for `OrderObjects`).
    Order(Vec<u32>),
    /// Arrange `(card_idx, dest_idx, position)`.
    Arrangement(Vec<(u32, u32, u32)>),
    /// A composite payment: mana-source indices + non-mana payment-option indices.
    Payment { mana: Vec<u32>, non_mana: Vec<u32> },
    /// A `PlayableAction` chosen at priority (follow-up sub-decisions come as their own reqs).
    Action(u32),
}

// ════════════════════════════════════════════════════════════════════════════════════════
// Supporting request types (§5)
// ════════════════════════════════════════════════════════════════════════════════════════

/// The in-progress cast/activation a sub-decision belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionRef(pub StackId);

/// Which ability of an object (index into its abilities).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityRef(pub u32);

/// One legal play at priority. Choosing a `Cast` spawns follow-up requests in CR 601.2 order:
/// `CastingTimeOptions` → `ChooseTargets` → `PayCost`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlayableAction {
    Cast { spell: ObjId, variant: CastVariant },
    Activate { source: ObjId, ability: AbilityRef },
    ActivateMana { source: ObjId, ability: AbilityRef },
    PlayLand { card: ObjId },
    /// Cast a **copy** of a prepared permanent's back-face spell (SoS Prepare DFCs). `source` is the
    /// prepared creature on the battlefield (carrying `Ability::Prepare{spell}`); casting mints a copy
    /// of the linked back-face spell, pays its mana cost through the normal cast pipeline (CR 707.12),
    /// and unprepares `source`. Offered only while `source` is prepared and at the back face's timing.
    CastPrepared { source: ObjId },
    /// Cast a spell from hand **without paying its mana cost**, using a once-per-turn free-cast
    /// permission granted by `source` (a permanent carrying a `StaticContribution::
    /// FreeCastFromHandOncePerTurn` static — Zaffai and the Tempests). Casting `spell` for
    /// [`CastVariant::WithoutPayingManaCost`] marks `source` used for the turn (`used_once_per_turn`).
    /// Offered only on the controller's turn, at `spell`'s own timing, while `source` is unused.
    CastFreeFromHand { source: ObjId, spell: ObjId },
    Special { kind: SpecialAction },
}

/// How a spell is being cast (mirrors GRE `ActionType` cast variants).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastVariant {
    Normal,
    Adventure,
    Mdfc,
    Left,
    Right,
    Omen,
    Prototype,
    WithoutPayingManaCost,
    /// Cast from hand for the card's warp cost (CR 702.x) — the spell is then exiled at the next
    /// end step and may be cast from exile later.
    Warp,
    /// Cast from the **graveyard** for the card's flashback cost (CR 702.34); the card is exiled as it
    /// resolves (or otherwise leaves the stack) instead of going anywhere else.
    Flashback,
    /// Cast from hand for the card's **miracle cost** (CR 702.94) — a fixed alternative cast cost read
    /// by `miracle_cost(card, caster)` (printed [`crate::effects::ability::Ability::Miracle`] or a
    /// granted [`crate::effects::ability::Ability::GrantMiracle`]). Offered only during the miracle
    /// reveal window ([`crate::stack::StackObjectKind::MiracleWindow`]).
    Miracle,
    /// Cast for the card's **overload cost** (CR 702.96, [`crate::effects::ability::Ability::Overload`]):
    /// the spell chooses no targets and every "target …" in its effect becomes "each …" (an engine-side
    /// `Target(spec)`→`ForEach` rewrite). Offered from the hand alongside the normal cast.
    Overload,
    /// Cast for the card's **alternative cost** (CR 118.9,
    /// [`crate::effects::ability::Ability::AlternativeCast`]): pay a non-mana cost (Daze — return an
    /// Island; Force of Will — pay 1 life + exile a blue card) rather than the mana cost. Offered from
    /// the hand alongside the normal cast, at the card's own timing.
    Alternative,
}

/// A special action (CR 116) — no stack, no response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpecialAction {
    PlayLand,
    TurnFaceUp(ObjId),
    EndContinuousEffect(ObjId),
}

/// One "target" requirement slot: its pre-filtered legal candidates and how many it takes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetSlot {
    /// Human/UI description (e.g. "target creature an opponent controls").
    pub description: String,
    /// Already filtered for protection/hexproof/ward/etc. — every entry is legal.
    pub legal: Vec<Target>,
    pub min: u32,
    pub max: u32,
}

/// One eligible attacker and the defenders it may attack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttackerOption {
    pub creature: ObjId,
    pub may_attack: Vec<Target>,
    /// Must attack if able (CR 508.1d).
    pub required: bool,
    pub attack_cost: Option<CostRequest>,
    pub may_exert: bool,
    pub may_enlist: bool,
}

/// One eligible blocker and the attackers it may legally block (evasion already applied).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockerOption {
    pub creature: ObjId,
    pub may_block: Vec<ObjId>,
    pub required: bool,
    pub block_cost: Option<CostRequest>,
}

/// One damage recipient + the lethal threshold for it (toughness − marked; deathtouch ⇒ 1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DamageSlot {
    pub recipient: Target,
    pub lethal: u32,
}

/// What an `OrderObjects` request is ordering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderKind {
    TriggersOnStack { controller: PlayerId },
    BlockersOf(ObjId),
    AttackersOf(ObjId),
    MoveToZone(Zone),
    CostSequence,
    SimultaneousSpellAbilities,
}

/// What must be paid for a `PayCost` request (engine-resolved concrete amounts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CostRequest {
    pub mana: Option<ManaCost>,
    pub components: Vec<CostComponentReq>,
}

/// A concrete non-mana cost component to pay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostComponentReq {
    PayLife(u32),
    Sacrifice { count: u32, description: String },
    Discard { count: u32 },
    Exile { count: u32, description: String },
    Tap,
    RemoveCounters { kind: CounterKind, n: u32 },
}

/// A mana source the player may fire to help pay (CR 605/602.2g).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManaSource {
    pub source: ObjId,
    pub ability: AbilityRef,
    /// Colors this source can produce (`Color::Colorless` for `{C}`).
    pub produces: Vec<Color>,
}

/// A non-mana payment option the engine enumerated as legal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaymentOption {
    TapPermanent(ObjId),
    Sacrifice(ObjId),
    Exile(ObjId),
    Discard(ObjId),
    PayLife(u32),
    RemoveCounter(ObjId, CounterKind),
}

/// One group in a `SelectFromGroups` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectGroup {
    pub label: String,
    pub options: Vec<ObjId>,
    pub min: u32,
    pub max: u32,
}

/// One applicable replacement/prevention effect to choose among (CR 616.1f).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplacementOption {
    pub source: ObjId,
    pub description: String,
}

/// A labeled option for `ChooseOption` (type/vote/keyword/face/name/…).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionLabel {
    pub label: String,
}

/// A mode option for `ChooseModes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeOption {
    pub label: String,
}

/// One cast-time optional/additional cost in a `CastingTimeOptions` request. The caller opts in
/// by index. (Value-bearing cast choices — X, modes, mana-type — are separate decisions; see
/// the `CastingTimeOptions` variant doc.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CastOption {
    pub label: String,
    /// `true` = an additional cost that must be paid; `false` = an optional cost (kicker-style).
    pub required: bool,
}

// ── reason / kind tags (let backends & RL heads route without re-deriving context) ──────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NumberReason {
    ChooseX,
    ChooseAnyAmount,
    CostReduction,
    KeywordCost,
    DieRoll,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SelectReason {
    Sacrifice,
    Destroy,
    Discard,
    DiscardToHandSize,
    BottomForMulligan,
    Search,
    Reveal,
    ScryStage,
    ActivateFromOpeningHand,
    ConvokeImprovise,
    Delve,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistributeReason {
    CombatDamage,
    DamageEffect,
    Counters,
    Shield,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArrangeReason {
    Scry,
    Surveil,
    Fateseal,
    LookAndArrange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptionReason {
    ChooseType,
    Vote,
    Protection,
    KeywordToGrant,
    NameCard,
    CardFace,
    Sector,
    Sprocket,
}

/// What a `Confirm` is asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfirmKind {
    OptionalTrigger(StackId),
    MayEffect,
    PayToPrevent,
    FlipCall,
    StaticApplication,
    Bid(u32),
    PutOnTop(ObjId),
    KeepHand,
    Generic,
}

// ════════════════════════════════════════════════════════════════════════════════════════
// GameEvent — the observe() push channel
// ════════════════════════════════════════════════════════════════════════════════════════

/// A public (or seat-visible) game event pushed to agents via [`Agent::observe`]. Maps to the
/// GRE `GameStateMessage` diff stream. A starter vocabulary; grows alongside the engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GameEvent {
    PhaseBegan { turn: u32, phase: Phase, active: PlayerId },
    DrewCards { player: PlayerId, count: u32 },
    LifeChanged { player: PlayerId, delta: i32, new_total: i32 },
    DamageDealt { target: Target, amount: u32, source: ObjId },
    SpellCast { spell: StackId, controller: PlayerId },
    /// One or more cards left `player`'s graveyard during a single effect resolution (the SoS Lorehold
    /// "whenever one or more cards leave your graveyard" trigger). Batched per resolution.
    LeftGraveyard { player: PlayerId },
    ObjectMoved { obj: ObjId, to: Zone },
    PermanentDied { obj: ObjId },
    /// One or more counters of `kind` were put on `obj` (CR 122). Fires once per counter-adding
    /// event (not per counter). Drives "whenever one or more +1/+1 counters are put on this
    /// creature …" (Pensive Professor / Berta).
    CountersPut { obj: ObjId, kind: CounterKind, count: u32 },
    Revealed { to: PlayerId, objects: Vec<ObjId> },
    ValueChosen { player: PlayerId, label: String, value: i64 },
    /// `object` became the target of a spell or ability controlled by `by` (CR 603.2/601.2c) —
    /// fired as targets are locked. `source` is the targeting spell/ability's stack id (so a Ward
    /// soft-counter can reference "that spell or ability"). Drives "becomes the target of …" triggers.
    Targeted { object: ObjId, by: PlayerId, source: StackId },
    /// Player `by` declared `attackers` as attacking creatures (CR 508.1). Drives "whenever you
    /// attack" (player-level, once) and "whenever this creature attacks" (per attacker) triggers.
    AttackersDeclared { attackers: Vec<ObjId>, by: PlayerId },
    /// One or more creatures `controller` controls dealt combat damage to a player this combat-damage
    /// step (CR 510.1c). Broadcast once per such controller per step — drives the batched
    /// `YouDealCombatDamageToPlayer` trigger (Killian's Confidence).
    CombatDamageToPlayerBy { controller: PlayerId },
    GameEnded { winner: Option<PlayerId> },
    /// `player`'s mana pool changed (mana produced or spent during payment/resolution). A *live-view*
    /// notification: it refreshes observers so the client shows floating mana mid-resolution
    /// (#59/#62), but is NOT recorded to the event log or replay (it would bloat them with churn).
    ManaPoolChanged { player: PlayerId },
}

// ════════════════════════════════════════════════════════════════════════════════════════
// RandomAgent — the reference backend (ENGINE_PLAN §6: first backend to implement)
// ════════════════════════════════════════════════════════════════════════════════════════

/// A backend that picks a uniformly-random *legal* option for every request. Because the engine
/// pre-enumerates only legal options, this can never make an illegal move — which is exactly the
/// property `RandomAgent` exists to prove. Headless and deterministic given its seed (uses the
/// engine's replayable [`Rng`](crate::rng::Rng)).
#[derive(Debug, Clone)]
pub struct RandomAgent {
    rng: crate::rng::Rng,
}

impl RandomAgent {
    pub fn new(seed: u64) -> Self {
        RandomAgent {
            rng: crate::rng::Rng::new(seed),
        }
    }

    /// A random index in `[0, n)`, or 0 if `n == 0`.
    fn idx(&mut self, n: usize) -> u32 {
        if n == 0 {
            0
        } else {
            self.rng.below(n as u64) as u32
        }
    }

    /// `k` distinct random indices in `[0, n)` where `min <= k <= max` (clamped to `n`).
    fn subset(&mut self, n: usize, min: u32, max: u32) -> Vec<u32> {
        let max = (max as usize).min(n);
        let min = (min as usize).min(max);
        let k = if max == min {
            min
        } else {
            min + (self.rng.below((max - min + 1) as u64) as usize)
        };
        let mut pool: Vec<u32> = (0..n as u32).collect();
        // Partial Fisher–Yates: take the first `k` after shuffling those positions.
        for i in 0..k {
            let j = i + (self.rng.below((n - i) as u64) as usize);
            pool.swap(i, j);
        }
        let mut out: Vec<u32> = pool.into_iter().take(k).collect();
        out.sort_unstable();
        out
    }

    /// Pick a legal value for a `ChooseNumber` request, honoring step/parity/forbidden.
    fn pick_number(
        &mut self,
        min: i64,
        max: i64,
        step: u32,
        forbidden: &[i64],
        no_even: bool,
        no_odd: bool,
    ) -> i64 {
        let step = step.max(1) as i64;
        let legal: Vec<i64> = (min..=max)
            .step_by(step as usize)
            .filter(|v| !forbidden.contains(v))
            .filter(|v| !(no_even && v % 2 == 0))
            .filter(|v| !(no_odd && v % 2 != 0))
            .collect();
        if legal.is_empty() {
            min
        } else {
            legal[self.rng.below(legal.len() as u64) as usize]
        }
    }
}

impl Agent for RandomAgent {
    fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
        match req {
            DecisionRequest::ChooseStartingPlayer { candidates } => {
                // Choose to go first (generally advantageous). Consumes no RNG, so seeded games
                // stay deterministic and decision streams are unperturbed.
                let me = candidates.iter().position(|&c| c == view.seat).unwrap_or(0);
                DecisionResponse::Index(me as u32)
            }
            // Keep every opening hand: a coin-flip mulligan is meaningless noise for self-play
            // (a real policy should decide this), and keeping consumes no RNG, so seeded games
            // stay deterministic. `Bool(false)` = keep; `Bool(true)` = mulligan.
            DecisionRequest::Mulligan { .. } => DecisionResponse::Bool(false),
            DecisionRequest::Priority { actions, can_pass } => {
                let want_pass = *can_pass && self.rng.below(2) == 0;
                if actions.is_empty() || want_pass {
                    DecisionResponse::Pass
                } else {
                    DecisionResponse::Action(self.idx(actions.len()))
                }
            }
            DecisionRequest::ChooseModes { modes, min, max, .. } => {
                DecisionResponse::Indices(self.subset(modes.len(), *min, *max))
            }
            DecisionRequest::ChooseNumber {
                min,
                max,
                step,
                forbidden,
                disallow_even,
                disallow_odd,
                ..
            } => DecisionResponse::Number(self.pick_number(
                *min,
                *max,
                *step,
                forbidden,
                *disallow_even,
                *disallow_odd,
            )),
            // CastingTimeOptions now carries only optional/additional costs (X/modes/mana-type are
            // their own decisions), so it is cleanly answered by `Indices`. Decline all optional
            // costs (a random agent that pays nothing extra is always legal).
            DecisionRequest::CastingTimeOptions { .. } => DecisionResponse::Indices(vec![]),
            DecisionRequest::ChooseTargets { slots, .. } => {
                let mut pairs = Vec::new();
                for (slot_idx, slot) in slots.iter().enumerate() {
                    for choice in self.subset(slot.legal.len(), slot.min, slot.max) {
                        pairs.push((slot_idx as u32, choice));
                    }
                }
                DecisionResponse::Pairs(pairs)
            }
            DecisionRequest::Distribute {
                among,
                total,
                min_each,
                ..
            } => {
                let mut amounts: Vec<(u32, u32)> =
                    (0..among.len() as u32).map(|i| (i, *min_each)).collect();
                let assigned: u32 = min_each * (among.len() as u32);
                let remainder = total.saturating_sub(assigned);
                if let Some(first) = amounts.first_mut() {
                    first.1 += remainder;
                }
                DecisionResponse::Amounts(amounts)
            }
            DecisionRequest::PayCost { mana_sources, .. } => DecisionResponse::Payment {
                mana: (0..mana_sources.len() as u32).collect(),
                non_mana: vec![],
            },
            DecisionRequest::DeclareAttackers { eligible } => {
                let mut pairs = Vec::new();
                for (i, opt) in eligible.iter().enumerate() {
                    if opt.may_attack.is_empty() {
                        continue;
                    }
                    if opt.required || self.rng.below(2) == 0 {
                        pairs.push((i as u32, self.idx(opt.may_attack.len())));
                    }
                }
                DecisionResponse::Pairs(pairs)
            }
            DecisionRequest::DeclareBlockers { .. } => DecisionResponse::Pairs(vec![]),
            DecisionRequest::AssignCombatDamage { total, .. } => {
                DecisionResponse::Amounts(vec![(0, *total)])
            }
            DecisionRequest::OrderObjects { items, .. } => {
                DecisionResponse::Order((0..items.len() as u32).collect())
            }
            DecisionRequest::SelectCards {
                from, min, max, ..
            } => DecisionResponse::Indices(self.subset(from.len(), *min, *max)),
            DecisionRequest::SelectFromGroups { groups, .. } => {
                let mut pairs = Vec::new();
                for (g, group) in groups.iter().enumerate() {
                    for choice in self.subset(group.options.len(), group.min, group.max) {
                        pairs.push((g as u32, choice));
                    }
                }
                DecisionResponse::Pairs(pairs)
            }
            DecisionRequest::ArrangeCards { cards, .. } => DecisionResponse::Arrangement(
                (0..cards.len() as u32).map(|i| (i, 0, i)).collect(),
            ),
            DecisionRequest::ChooseReplacement { applicable, .. } => {
                DecisionResponse::Index(self.idx(applicable.len()))
            }
            DecisionRequest::ChooseCounterType { options } => {
                DecisionResponse::Index(self.idx(options.len()))
            }
            DecisionRequest::ChooseOption {
                options, min, max, ..
            } => DecisionResponse::Indices(self.subset(options.len(), *min, *max)),
            DecisionRequest::ChooseColor { allowed, min, max } => {
                DecisionResponse::Indices(self.subset(allowed.len(), *min, *max))
            }
            DecisionRequest::Confirm { .. } => DecisionResponse::Bool(self.rng.below(2) == 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_view() -> PlayerView {
        PlayerView {
            seat: PlayerId(0),
            turn: 1,
            active_player: PlayerId(0),
            phase: Phase::PrecombatMain,
            priority_player: Some(PlayerId(0)),
            players: vec![],
            me: PlayerPrivateView {
                hand: vec![],
                known_library: vec![],
                revealed_to_me: vec![],
            },
            battlefield: vec![],
            stack: vec![],
            combat: None,
            stops: None,
        }
    }

    #[test]
    fn random_agent_priority_is_legal() {
        let view = sample_view();
        let mut agent = RandomAgent::new(7);
        let req = DecisionRequest::Priority {
            actions: vec![
                PlayableAction::PlayLand { card: ObjId(1) },
                PlayableAction::Cast {
                    spell: ObjId(2),
                    variant: CastVariant::Normal,
                },
            ],
            can_pass: true,
        };
        // Over many draws every response is either Pass or an in-range Action index.
        for _ in 0..200 {
            match agent.decide(&view, &req) {
                DecisionResponse::Pass => {}
                DecisionResponse::Action(i) => assert!((i as usize) < 2),
                other => panic!("unexpected priority response: {other:?}"),
            }
        }
    }

    #[test]
    fn random_agent_choose_number_respects_constraints() {
        let view = sample_view();
        let mut agent = RandomAgent::new(99);
        // Legal odd values in [1,10] excluding 7: {1,3,5,9}.
        let req = DecisionRequest::ChooseNumber {
            reason: NumberReason::ChooseX,
            min: 1,
            max: 10,
            step: 1,
            forbidden: vec![7],
            disallow_even: true,
            disallow_odd: false,
        };
        for _ in 0..200 {
            match agent.decide(&view, &req) {
                DecisionResponse::Number(n) => {
                    assert!([1, 3, 5, 9].contains(&n), "illegal number chosen: {n}");
                }
                other => panic!("unexpected number response: {other:?}"),
            }
        }
    }

    #[test]
    fn random_agent_targets_in_range() {
        let view = sample_view();
        let mut agent = RandomAgent::new(3);
        let req = DecisionRequest::ChooseTargets {
            for_action: ActionRef(StackId(0)),
            source: None,
            slots: vec![TargetSlot {
                description: "target creature".into(),
                legal: vec![Target::Object(ObjId(10)), Target::Object(ObjId(11))],
                min: 1,
                max: 1,
            }],
        };
        match agent.decide(&view, &req) {
            DecisionResponse::Pairs(pairs) => {
                assert_eq!(pairs.len(), 1);
                let (slot, choice) = pairs[0];
                assert_eq!(slot, 0);
                assert!((choice as usize) < 2);
            }
            other => panic!("unexpected targets response: {other:?}"),
        }
    }

    #[test]
    fn random_agent_is_deterministic_for_seed() {
        let view = sample_view();
        let req = DecisionRequest::Confirm {
            kind: ConfirmKind::MayEffect,
        };
        let mut a = RandomAgent::new(123);
        let mut b = RandomAgent::new(123);
        for _ in 0..50 {
            assert_eq!(a.decide(&view, &req), b.decide(&view, &req));
        }
    }

    #[test]
    fn request_response_roundtrip_serde() {
        // The boundary types must serialize (the §1.1 GRE-server / socket contract).
        let req = DecisionRequest::DeclareAttackers {
            eligible: vec![AttackerOption {
                creature: ObjId(5),
                may_attack: vec![Target::Player(PlayerId(1))],
                required: false,
                attack_cost: None,
                may_exert: false,
                may_enlist: false,
            }],
        };
        let json = serde_json::to_string(&req).expect("serialize request");
        let back: DecisionRequest = serde_json::from_str(&json).expect("deserialize request");
        assert_eq!(format!("{req:?}"), format!("{back:?}"));
    }
}

/// Inline snapshot ("expect") tests that pin the **serialized wire shape** of representative
/// boundary values — the §1.1 contract the GRE-server / socket backends serialize over. These
/// double as documentation: the JSON below is exactly what crosses the boundary. Regenerate
/// with `UPDATE_EXPECT=1 cargo test`.
#[cfg(test)]
mod wire_snapshots {
    use super::*;
    use expect_test::expect;

    fn json(value: &impl Serialize) -> String {
        serde_json::to_string_pretty(value).unwrap()
    }

    #[test]
    fn priority_request_wire_shape() {
        let req = DecisionRequest::Priority {
            actions: vec![
                PlayableAction::PlayLand { card: ObjId(11) },
                PlayableAction::Cast {
                    spell: ObjId(12),
                    variant: CastVariant::Normal,
                },
            ],
            can_pass: true,
        };
        expect![[r#"
            {
              "Priority": {
                "actions": [
                  {
                    "PlayLand": {
                      "card": 11
                    }
                  },
                  {
                    "Cast": {
                      "spell": 12,
                      "variant": "Normal"
                    }
                  }
                ],
                "can_pass": true
              }
            }"#]]
        .assert_eq(&json(&req));
    }

    #[test]
    fn choose_number_request_wire_shape() {
        // X with forbidden values + parity — the WHITEBOARD §2.6 "forbidden X" / NumericInputReq
        // constraint encoding, pinned.
        let req = DecisionRequest::ChooseNumber {
            reason: NumberReason::ChooseX,
            min: 1,
            max: 10,
            step: 1,
            forbidden: vec![7],
            disallow_even: true,
            disallow_odd: false,
        };
        expect![[r#"
            {
              "ChooseNumber": {
                "reason": "ChooseX",
                "min": 1,
                "max": 10,
                "step": 1,
                "forbidden": [
                  7
                ],
                "disallow_even": true,
                "disallow_odd": false
              }
            }"#]]
        .assert_eq(&json(&req));
    }

    #[test]
    fn choose_targets_request_wire_shape() {
        let req = DecisionRequest::ChooseTargets {
            for_action: ActionRef(StackId(3)),
            source: None,
            slots: vec![TargetSlot {
                description: "target creature".into(),
                legal: vec![Target::Object(ObjId(20)), Target::Player(PlayerId(1))],
                min: 1,
                max: 1,
            }],
        };
        expect![[r#"
            {
              "ChooseTargets": {
                "for_action": 3,
                "source": null,
                "slots": [
                  {
                    "description": "target creature",
                    "legal": [
                      {
                        "Object": 20
                      },
                      {
                        "Player": 1
                      }
                    ],
                    "min": 1,
                    "max": 1
                  }
                ]
              }
            }"#]]
        .assert_eq(&json(&req));
    }

    #[test]
    fn response_wire_shapes() {
        expect![[r#"
            {
              "Action": 0
            }"#]]
        .assert_eq(&json(&DecisionResponse::Action(0)));

        expect![[r#"
            {
              "Amounts": [
                [
                  0,
                  2
                ],
                [
                  1,
                  1
                ]
              ]
            }"#]]
        .assert_eq(&json(&DecisionResponse::Amounts(vec![(0, 2), (1, 1)])));

        expect![[r#"
            {
              "Payment": {
                "mana": [
                  0,
                  1
                ],
                "non_mana": []
              }
            }"#]]
        .assert_eq(&json(&DecisionResponse::Payment {
            mana: vec![0, 1],
            non_mana: vec![],
        }));
    }
}
