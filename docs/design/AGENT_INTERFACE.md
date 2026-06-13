# The Agent Interface: One Decision Boundary

> **Status:** Foundational design decision (DESIGN ONLY — no crate code yet; the
> implementation lands in `crates/mtg-core/src/agent.rs` + `crates/mtg-core/src/effects/`
> under board task #4).
>
> **Read first:** `docs/design/WHITEBOARD_MODEL.md` (esp. §2.1 whiteboard/Action, §2.3 the
> IR, §2.6 decisions-carry-constraints), `docs/plans/ENGINE_PLAN.md` §6 (the decision
> boundary), `docs/plans/GYM_PLAN.md` §3 (the agent interface + action masking),
> `docs/plans/DECOMPILE_PLAN.md` §1/§5 (the recovered MTGA GRE request catalog).
>
> This document specifies **the single seam through which every player choice flows.** It is
> the project's "easy switch" goal made concrete: scripted AI, Python RL policy, and a future
> MTGA-client driver are interchangeable `impl Agent`, and the engine never changes when you
> swap them.

---

## 0. The laws (non-negotiable design constraints)

These four properties define the boundary. Everything below is in service of them.

1. **One trait, one request enum, one response enum.** Every decision the engine can ever
   ask is a variant of `DecisionRequest`. There is no second decision path, no per-card
   callback, no out-of-band prompt. (CLAUDE.md "one decision boundary".)
2. **The engine pre-enumerates the legal options. Masking is the engine's job.** Each
   request variant carries the *complete, already-legal* option set (legality = rules +
   timing + targeting + mana availability + restrictions/requirements). An agent can only
   return an index/selection into that set. It is *structurally impossible* to choose an
   illegal action. → RL gets action masks for free; scripted/human agents can't cheat.
3. **Agents see a `PlayerView`, never `GameState`.** The view is information-filtered per
   seat: hidden zones masked, opponent hand as a count, library order hidden, face-down
   cards hidden from players who may not see them. This is the *only* correct place to
   enforce hidden information (rules-correctness) and it is required for RL (no info leak).
4. **The request set is a superset of both proven granularities, and is 1:1-translatable to
   the GRE wire.** It covers every Forge `PlayerController` decision method (the battle-tested
   107-method interface) **and** every MTGA GRE `*Req` in the recovered catalog (§6 proves the
   coverage). The three concrete non-engine backends are therefore all *implementation swaps,
   no engine changes*:
   - **`PyAgent`** (Gym/RL) and **`ScriptedAgent`** implement `decide` directly.
   - **A GRE-protocol server** fronting (a) a from-scratch **web client** and (b) the **real
     MTGA client** (task #5 / `docs/plans/CLIENT_PLAN.md`). It maps `DecisionRequest →
     GreToClientMessage` and `DecisionResponse/GameAction → ClientToGreMessage`, and streams
     `PlayerView` deltas as `GameStateMessage` (via `observe`, §1.1). Because every variant is
     a superset of a GRE `*Req` and the types serialize cleanly (§1.1), this mapping is
     **lossless** — the server is a translation layer, not a reinterpretation.

   See §1.1 for the serialization/translatability contract that makes the GRE server a pure
   adapter.

---

## 1. The trait

```rust
/// One per seat. The engine calls `decide` whenever that seat must choose.
/// This is the entire decision surface of the engine — nothing else asks a player anything.
pub trait Agent {
    /// The engine has reached a choice point for `view.seat`. `req` enumerates the
    /// *complete legal option set* plus any constraints. Return a response that selects
    /// among those options. The engine validates the response is in-range; an out-of-range
    /// or malformed response is an engine-level error (a correct agent never produces one).
    fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse;

    /// Push-only notification: a public reveal, a die-roll result, a value chosen by an
    /// opponent, a zone change — anything the seat should learn but need not answer.
    /// Default no-op. Mirrors Forge `PlayerController.reveal()/notifyOfValue()` and the
    /// GRE `GameStateMessage` diff / `RevealHandReq` stream.
    fn observe(&mut self, _view: &PlayerView, _ev: &GameEvent) {}
}
```

Notes:
- `decide` is **synchronous and total**: the engine blocks on it and expects exactly one
  response. The async story (RL env *pulls* requests rather than the engine pushing into
  Python) lives at the PyO3 boundary (GYM_PLAN §2/§3): `PyAgent::decide` is implemented as a
  one-shot channel yield so `step_until_decision`/`apply_decision` line up with Gym `step`
  semantics. The *trait* stays synchronous; transport is an implementation detail.
- `&mut self`: agents may hold internal state (RNG, policy handle, socket, search tree).
- The engine passes a fresh `&PlayerView` each call because the view changes between
  decisions; agents must not cache it across calls.

### 1.1 Serialization & 1:1 GRE translatability (the GRE-server contract)

`DecisionRequest`, `DecisionResponse`, `PlayerView`, `GameEvent`, and every supporting type
in §5 **derive `serde::{Serialize, Deserialize}`**. This is what lets a non-in-process backend
(the GRE-protocol server fronting the web client and the real MTGA client; also a Forge-style
`SocketAgent`) sit behind the trait without the engine knowing. The contract the GRE server
relies on:

- **Lossless, mechanical mapping in both directions.** Each `DecisionRequest` variant maps
  onto exactly one GRE `*Req` (table §6.1); each `DecisionResponse` maps onto the matching
  GRE `*Resp` / submitted `GameAction` (`ActionType` + payload). The mapping is *table-driven*
  (no game logic in the adapter) and the variant set is a strict superset, so no engine
  decision is unrepresentable on the wire.
- **Indices resolve back to concrete GRE object refs via the request's option vectors.** Our
  `DecisionResponse` is index-based (§4) while GRE submits concrete object/zone ids. The
  adapter reconstructs the GRE payload by indexing into the *same enumerated option vectors*
  the request carried (e.g. `Indices([0,2])` over `DeclareAttackers.eligible` → the GRE
  attacker→defender map). This is lossless precisely because the engine pre-enumerated the
  legal set (law #2) — the option vectors are the shared vocabulary both sides index.
- **`PlayerView` deltas ↔ `GameStateMessage`.** `observe(view, ev)` is the push channel the
  server turns into GRE `GameStateMessage` (Full/Diff) frames; `PlayerView`'s object/zone
  shape (§2) is designed to carry what a `GameObject`/`ZoneInfo`/`PlayerInfo` diff needs.
- **Field-exactness is pinned against decompile's schema.** The *variant set* is fixed (it is
  already a superset of the recovered `GREMessageType` catalog); the remaining work when
  `mtga-re/schema/gre_schema.json` lands is to confirm field-level names/numbers so the
  adapter's per-variant struct mapping is exact (§9 tracks the open field-shape questions).

> Net: the web client and the real MTGA client are **the same backend** — clients of one GRE
> server that is a thin, table-driven (de)serialization adapter over this boundary. No engine
> change distinguishes "human at a web UI", "RL policy", or "real MTGA client".

---

## 2. `PlayerView` — the information-filtered state

The agent's entire window onto the game. Computed from full `GameState` by a single masking
function (`GameState::view_for(seat) -> PlayerView`) — the one correct place to enforce
hidden information. Sketch (fields illustrative, not final — coordinate with `engine` on the
exact `GameState` shape):

```rust
pub struct PlayerView {
    pub seat: PlayerId,               // whose view this is
    pub turn: u32,
    pub active_player: PlayerId,
    pub phase: Phase,                 // reuse the skeleton's Phase vocabulary (types.rs)
    pub step_has_priority: bool,
    pub priority_player: Option<PlayerId>,

    pub players: Vec<PlayerPublicView>,   // one per seat, including self (public facts)
    pub me: PlayerPrivateView,            // self-only: full hand, known library tops, etc.

    pub battlefield: Vec<ObjView>,        // all permanents (public), with computed chars
    pub stack: Vec<StackObjView>,         // spells + abilities on the stack
    pub combat: Option<CombatView>,       // attackers/blockers/assignments when in combat

    pub recent_events: Vec<GameEvent>,    // public log slice since last decision (for RL/obs)
}

pub struct PlayerPublicView {
    pub player: PlayerId,
    pub life: i32,
    pub poison: u32,
    pub hand_count: u32,                  // opponent hand = count only (hidden)
    pub library_count: u32,               // order hidden
    pub graveyard: Vec<ObjView>,          // public zone, fully visible
    pub exile_public: Vec<ObjView>,       // face-up exile only
    pub mana_pool: ManaPool,
    pub counters: CounterBag,             // e.g. energy, experience
}

pub struct PlayerPrivateView {
    pub hand: Vec<ObjView>,               // full detail — this seat's own hand
    pub known_library: Vec<Option<ObjView>>, // tops revealed by scry/Sylvan Library etc.
    pub revealed_to_me: Vec<ObjView>,     // cards an effect has shown only to this seat
}

/// A single object as this seat is allowed to perceive it. Hidden/face-down objects
/// collapse to a masked stub (id + zone + controller only).
pub enum ObjView {
    Visible {
        id: ObjId,
        chars: Characteristics,           // post-layer computed view (CR 613 output)
        controller: PlayerId,
        owner: PlayerId,
        zone: Zone,
        status: Status,                   // tapped/flipped/face-up/phased (CR 110.5)
        counters: CounterBag,
        damage_marked: u32,
        attachments: Vec<ObjId>,
        // combat role, summoning sickness, etc. surfaced as needed
    },
    Hidden { id: ObjId, zone: Zone, controller: PlayerId }, // face-down/opponent-hidden
}
```

Hidden-information rules the masking function enforces (all rules-mandated, all RL-required):
opponent hand → count only; library → count + order hidden (except cards this seat has been
shown); face-down permanents/exile → `Hidden` stub to players without permission; a spell's
chosen-but-secret info per its rules. Anything an opponent reveals arrives via `observe`.

> **Why a typed view, not raw state:** RL needs a stable, leak-free observation; the rules
> need hidden info enforced exactly once; a socket/MTGA backend needs to serialize only what
> its seat may legally know. One masking function satisfies all three.

---

## 3. `DecisionRequest` — the enumerated, masked choice set

The closed set of every choice the engine can ask. Each variant **pre-enumerates legal
options** (law #2). Per-variant doc comments name the Forge `PlayerController` method(s) and
the MTGA GRE `*Req` it subsumes (the superset proof is the table in §6).

```rust
pub enum DecisionRequest {
    // ─── Game setup ───────────────────────────────────────────────────────────────
    /// Pick who takes the first turn.
    /// Forge: chooseStartingPlayer.            GRE: ChooseStartingPlayerReq.
    ChooseStartingPlayer { candidates: Vec<PlayerId> },

    /// London mulligan keep-or-mulligan decision. If the agent keeps after N mulligans,
    /// the engine immediately issues a `SelectCards{ reason: BottomForMulligan, .. }` for
    /// the N cards to put on the bottom.
    /// Forge: mulliganKeepHand / londonMulliganReturnCards / confirmMulliganScry.
    /// GRE:   MulliganReq (+ MulliganResp). (Field shape pending decompile — see §9.)
    Mulligan { hand: Vec<ObjId>, mulligans_taken: u32, will_bottom_if_kept: u32 },

    // ─── The priority loop (the hot path; most decisions are this) ──────────────────
    /// The active-priority player may cast/activate/play-land/take-a-special-action/pass.
    /// Every legal play is pre-enumerated as a `PlayableAction`. `can_pass` is almost
    /// always true (it is false only mid-cast where the rules forbid passing).
    /// Forge: getAbilityToPlay / chooseSpellAbilityToPlay / playChosenSpellAbility
    ///        / playSaFromPlayEffect.
    /// GRE:   ActionsAvailableReq (+ ActionType Pass/Play/Cast*/Activate/Activate_Mana/
    ///        Special/Special_TurnFaceUp).
    Priority { actions: Vec<PlayableAction>, can_pass: bool },

    // ─── Casting/activation sub-decisions (601.2 / 602.2) ───────────────────────────
    /// Choose modes for a modal spell/ability (CR 700.2). `min`/`max` bound the count;
    /// `allow_repeat` for "choose one or more, you may choose the same mode" style.
    /// Forge: chooseModeForAbility.            GRE: part of CastingTimeOptionsReq.
    ChooseModes { for_action: ActionRef, modes: Vec<ModeOption>, min: u32, max: u32, allow_repeat: bool },

    /// Choose a number: value of X, cost-reduction amount, keyword-cost N, "choose a
    /// number" effects. `forbidden` realizes WHITEBOARD_MODEL §2.6 (X with forbidden
    /// values). The legal set is `[min,max] \ forbidden`.
    /// Forge: announceRequirements (X) / chooseNumber / chooseNumberForCostReduction
    ///        / chooseNumberForKeywordCost.
    /// GRE:   NumericInputReq. (forbidden/min/max field shape pending decompile — §9.)
    ChooseNumber { reason: NumberReason, min: i64, max: i64, forbidden: Vec<i64> },

    /// Opt into/out of optional costs and pick cast-time options (kicker, buyback,
    /// additional/alternative costs, Phyrexian/hybrid payment choices). Often the engine
    /// decomposes this into `Confirm`/`ChooseNumber`/`ChooseModes` sub-steps; this variant
    /// exists for backends that prefer the batched GRE shape.
    /// Forge: chooseOptionalCosts.             GRE: CastingTimeOptionsReq (CastingTimeOptionType).
    CastingTimeOptions { for_action: ActionRef, options: Vec<CastOption> },

    /// Choose the targets for one action. One `TargetSlot` per instance of the word
    /// "target"; each slot carries its own pre-filtered legal candidate list and min/max.
    /// Forge: chooseTargetsFor / chooseNewTargetsFor / chooseTarget
    ///        / chooseSingleEntityForEffect / chooseEntitiesForEffect.
    /// GRE:   SelectTargetsReq (+ SubmitTargetsResp).
    ChooseTargets { for_action: ActionRef, slots: Vec<TargetSlot> },

    /// Divide/distribute an amount among recipients (damage division, counters, shield).
    /// Each recipient gets ≥ `min_each` (usually 1, CR 601.2d); division locks at this step.
    /// Forge: divideShield / divide-damage paths.   GRE: DistributionReq.
    Distribute { reason: DistributeReason, among: Vec<Target>, total: u32, min_each: u32, max_each: Option<u32> },

    /// Pay a cost: which mana to spend, which permanents to tap/sacrifice/exile for
    /// convoke/improvise/delve, life, discards-as-cost, etc. The engine pre-computes the
    /// legal payment methods; mana abilities are enumerated as `ManaSource`s (CR 605/602.2g).
    /// Forge: payManaCost / payCombatCost / payCostToPreventEffect / confirmPayment /
    ///        specifyManaCombo / chooseManaFromPool / chooseCardsForConvokeOrImprovise /
    ///        chooseCardsToDelve / orderCosts.
    /// GRE:   PayCostsReq (+ ActionType Make_Payment/Activate_Mana/FloatMana/Special_Payment).
    PayCost { cost: CostRequest, mana_sources: Vec<ManaSource>, non_mana: Vec<PaymentOption> },

    // ─── Combat (506–511) ───────────────────────────────────────────────────────────
    /// Declare attackers. Each `AttackerOption` lists which defenders that creature may
    /// attack and its restrictions (can't-attack) / requirements (must-attack), plus any
    /// attack cost and exert/enlist opt-ins. The engine enforces the "obey the maximum
    /// satisfiable requirement set" rule (CR 508.1d).
    /// Forge: declareAttackers / exertAttackers / enlistAttackers.
    /// GRE:   DeclareAttackersReq (+ SubmitAttackersResp).
    DeclareAttackers { eligible: Vec<AttackerOption> },

    /// Declare blockers. Each `BlockerOption` lists which attackers that creature may
    /// legally block (evasion/restrictions already applied, CR 509.1b) and any block cost.
    /// Forge: declareBlockers.                 GRE: DeclareBlockersReq (+ SubmitBlockersResp).
    DeclareBlockers { eligible: Vec<BlockerOption>, attackers: Vec<ObjId> },

    /// Assign combat damage from one attacker/blocker among its recipients in the chosen
    /// damage-assignment order. The engine supplies the lethal threshold per recipient
    /// (toughness − marked, deathtouch ⇒ 1) and whether trample/excess applies (CR 510.1c-e).
    /// Forge: assignCombatDamage.              GRE: AssignDamageReq (+ AssignDamageConfirmation).
    AssignCombatDamage { source: ObjId, recipients: Vec<DamageSlot>, total: u32,
                         deathtouch: bool, trample_to: Option<Target> },

    // ─── Ordering ────────────────────────────────────────────────────────────────────
    /// Order a set of objects: triggers on the stack (APNAP, CR 603.3b), an attacker's
    /// blockers (damage-assignment order), a blocker's attackers, cards moved to a zone,
    /// or a cost sequence. `kind` disambiguates.
    /// Forge: orderBlockers / orderBlocker / orderAttackers / orderMoveToZoneList /
    ///        orderCosts / orderAndPlaySimultaneousSa.
    /// GRE:   OrderReq (+ OrderResp) / OrderCombatDamageReq (+ OrderDamageConfirmation).
    OrderObjects { kind: OrderKind, items: Vec<ObjId> },

    // ─── Selection from a set ─────────────────────────────────────────────────────────
    /// Choose `min..=max` objects from a pre-filtered set for some effect: sacrifice,
    /// destroy, discard (incl. discard-to-max-hand-size), search a library/zone, reveal,
    /// scry/surveil staging, choose-creatures-for-effect, activate-from-opening-hand, etc.
    /// `reason` carries the context so a backend/RL head can route.
    /// Forge: choosePermanentsToSacrifice / choosePermanentsToDestroy / chooseCardsForEffect
    ///        / chooseEntitiesForEffect / chooseCardsToDiscardFrom /
    ///        chooseCardsToDiscardToMaximumHandSize / chooseCardsToRevealFromHand /
    ///        chooseSaToActivateFromOpeningHand / chooseCardsToDelve.
    /// GRE:   SelectNReq / SearchReq / RevealHandReq.
    SelectCards { reason: SelectReason, from: Vec<ObjId>, min: u32, max: u32, filter: CardFilter },

    /// Choose from multiple distinct groups at once (e.g. fetch one of each type, or pick
    /// from several piles). Each group has its own min/max.
    /// Forge: chooseCardsForEffectMultiple.    GRE: SelectNGroupReq / SelectFromGroupsReq /
    ///        SearchFromGroupsReq / GroupReq.
    SelectFromGroups { reason: SelectReason, groups: Vec<SelectGroup> },

    /// Stage the top N cards of a library into ordered destinations: scry (top/bottom),
    /// surveil (top/graveyard), fateseal (opponent), generic "look at top N, arrange".
    /// Forge: arrangeForScry / arrangeForSurveil / willPutCardOnTop / orderMoveToZoneList.
    /// GRE:   (part of SelectN / Scry/Surveil prompts — pending decompile, §9).
    ArrangeCards { reason: ArrangeReason, cards: Vec<ObjId>, destinations: Vec<ZoneDest> },

    /// Choose which replacement/prevention effect (or interacting static) applies to an
    /// event when several could (CR 616.1f), then the engine re-checks and may re-ask.
    /// Forge: chooseSingleReplacementEffect / confirmReplacementEffect /
    ///        chooseSingleStaticAbility.
    /// GRE:   SelectReplacementReq.
    ChooseReplacement { event: EventDigest, applicable: Vec<ReplacementOption> },

    /// Choose a counter kind (proliferate, "remove a counter", etc.).
    /// Forge: chooseCounterType.               GRE: SelectCountersReq.
    ChooseCounterType { options: Vec<CounterKind> },

    // ─── Small typed choices ──────────────────────────────────────────────────────────
    /// Choose one/several from a labeled option list that isn't a target or a card:
    /// a creature/land type, a vote, a sector/sprocket, a protection quality, a keyword to
    /// grant, a named card (StringInput collapses to a pick over a card-name vocabulary),
    /// a card face/state for an MDFC/adventure.
    /// Forge: chooseSomeType / vote / chooseSector / chooseSprocket / chooseProtectionType
    ///        / chooseKeywordForPump / chooseCardName / chooseSingleCardFace
    ///        / chooseSingleCardState / chooseSingleSpellForEffect / chooseSpellAbilitiesForEffect.
    /// GRE:   PromptReq / StringInputReq / "choose option from list" prompts.
    ChooseOption { reason: OptionReason, options: Vec<OptionLabel>, min: u32, max: u32 },

    /// Choose color(s). Separate from ChooseOption so RL gets a clean 5/6-way head.
    /// Forge: chooseColor / chooseColorAllowColorless / chooseColors.
    /// GRE:   part of choose-option-from-list.
    ChooseColor { allowed: Vec<Color>, min: u32, max: u32 },

    /// A yes/no (or this-or-that binary). `kind` carries what's being confirmed: an
    /// optional trigger, a "may" effect, pay-to-prevent, a coin/flip call, a static-ability
    /// application, a bid, put-on-top.
    /// Forge: confirmAction / confirmTrigger / confirmReplacementEffect (boolean form) /
    ///        confirmStaticApplication / confirmBidAction / chooseBinary / chooseFlipResult
    ///        / willPutCardOnTop / confirmMulliganScry / confirmPayment.
    /// GRE:   PromptReq / CHOOSE_BINARY / OptionalActionMessage.
    Confirm { kind: ConfirmKind },
}
```

### 3.1 Multi-select & autoregression note

The engine variants carry the *full batched* choice (e.g. `DeclareAttackers` with all
attackers at once) — this matches both Forge (`declareAttackers` fills the whole `Combat`)
and GRE (`DeclareAttackersReq` → one `SubmitAttackersResp`). The RL action-space layer
(GYM_PLAN §4) may *internally* decompose a batched multi-select into autoregressive
single-index sub-steps for a flat PPO action space; that is a `PyAgent`-side concern and does
**not** change the engine boundary. Keeping the engine request batched is what preserves the
1:1 GRE alignment that makes `MtgaClientAgent` a pure adapter.

---

## 4. `DecisionResponse`

Responses are **selections into the request's option vectors**, plus payloads for the few
variants that need structured data (damage split, distribution, ordering). Because the engine
enumerated only legal options, any in-range selection is automatically legal.

```rust
pub enum DecisionResponse {
    /// Pass priority (only valid when `Priority { can_pass: true }`).
    Pass,

    /// Pick exactly one option by index (Priority action, single target, single mode,
    /// starting player, replacement, counter type, single ChooseOption, ...).
    Index(u32),

    /// Pick a subset by indices (modes, cards, attackers, colors, multi-target).
    Indices(Vec<u32>),

    /// A number (X, cost reduction, generic "choose a number"). Engine checks ∈ legal set.
    Number(i64),

    /// Yes/no for `Confirm`.
    Bool(bool),

    /// Pairs: (selector_idx, target_idx). Blocker→attacker assignment, target→slot maps.
    Pairs(Vec<(u32, u32)>),

    /// Distribution: (recipient_idx, amount). Sums to the request's `total`; each ≥ min_each.
    /// Used by `Distribute` and `AssignCombatDamage`.
    Amounts(Vec<(u32, u32)>),

    /// A permutation of the request's `items` indices (for `OrderObjects`).
    Order(Vec<u32>),

    /// Arrange: per-card destination assignment, (card_idx, dest_idx, position).
    Arrangement(Vec<(u32, u32, u32)>),

    /// A composite payment: which mana sources to fire (by index) + which non-mana
    /// payment options (by index). Used by `PayCost`.
    Payment { mana: Vec<u32>, non_mana: Vec<u32> },

    /// A `PlayableAction` chosen at priority, when it needs no further inline payload
    /// (targets/cost/modes follow as their own subsequent `DecisionRequest`s).
    Action(u32),
}
```

**Validation contract:** the engine validates each response against the request it answers
(in-range indices, count ∈ `min..=max`, amounts sum to `total`, permutation is complete). A
violation is an *engine/agent-implementation bug*, not a game event — surfaced as
`EngineError::IllegalResponse`, never silently coerced. (This is the fix for Forge's
`Box(10,)`-modulo hack that ignored legality; GYM_PLAN §3.)

---

## 5. Supporting types (sketches)

Concrete enough to implement against; field-exact details settle in task #4 alongside the
real `GameState`. These reuse the skeleton vocabulary (`PlayerId`, `Color`, `Phase`,
`ManaPool` from `src/types.rs`) and introduce the stable-identity types the whiteboard model
calls for.

```rust
/// Stable per-object identity (WHITEBOARD_MODEL §2.5). A zone change generally mints a NEW
/// ObjId (CR 400.7); continuous effects/counters do not follow unless a rule says so.
/// This supersedes the skeleton's split CardId/PermanentId with one identity space.
pub struct ObjId(pub u32);
pub struct StackId(pub u32);     // position-independent handle to a stack object

pub enum Target {
    Player(PlayerId),
    Object(ObjId),               // permanent, or a card in a public zone
    Stack(StackId),              // a spell or ability on the stack
}

/// One legal play at priority. The engine builds one per castable spell, activatable
/// ability, playable land, and special action. Choosing it may spawn follow-up requests
/// (ChooseModes → ChooseTargets → ChooseNumber(X) → PayCost), mirroring CR 601.2b–h.
pub enum PlayableAction {
    Cast       { spell: ObjId, variant: CastVariant },   // CastVariant ~ ActionType Cast*/adventure/MDFC
    Activate   { source: ObjId, ability: AbilityRef },
    ActivateMana { source: ObjId, ability: AbilityRef }, // mana abilities (CR 605)
    PlayLand   { card: ObjId },
    Special    { kind: SpecialAction },                  // CR 116: turn face up, etc.
}

/// One "target" word's slot: its pre-filtered legal candidates and how many it takes.
pub struct TargetSlot {
    pub description: TargetKind,   // "target creature", "any target", ...
    pub legal: Vec<Target>,        // already filtered (protection/hexproof/ward-applied)
    pub min: u32,
    pub max: u32,
}

pub struct AttackerOption {
    pub creature: ObjId,
    pub may_attack: Vec<Target>,   // legal defenders (player / planeswalker / battle)
    pub required: bool,            // must attack if able (CR 508.1d)
    pub attack_cost: Option<CostRequest>,
    pub may_exert: bool,
    pub may_enlist: bool,
}

pub struct BlockerOption {
    pub creature: ObjId,
    pub may_block: Vec<ObjId>,     // attackers this creature may legally block (evasion applied)
    pub required: bool,
    pub block_cost: Option<CostRequest>,
}

pub struct DamageSlot { pub recipient: Target, pub lethal: u32 } // lethal threshold supplied

pub enum OrderKind {
    TriggersOnStack { controller: PlayerId },
    BlockersOf(ObjId),
    AttackersOf(ObjId),
    MoveToZone(Zone),
    CostSequence,
    SimultaneousSpellAbilities,
}

pub struct CostRequest { pub mana: Option<ManaCost>, pub additional: Vec<CostComponent> }
pub struct ManaSource  { pub source: ObjId, pub ability: AbilityRef, pub produces: ManaSpec }
pub enum   PaymentOption { TapPermanent(ObjId), Sacrifice(ObjId), Exile(ObjId),
                           Discard(ObjId), PayLife(u32), RemoveCounter(ObjId, CounterKind) }

// Reason/label tags let backends & RL heads route without re-deriving context.
pub enum NumberReason { ChooseX, CostReduction, KeywordCost(/* keyword */), Generic }
pub enum SelectReason { Sacrifice, Destroy, Discard, DiscardToHandSize, Search, Reveal,
                        ScryStage, ActivateFromOpeningHand, ConvokeImprovise, Delve, Generic }
pub enum DistributeReason { CombatDamage, DamageEffect, Counters, Shield }
pub enum ArrangeReason { Scry, Surveil, Fateseal, LookAndArrange }
pub enum OptionReason { ChooseType, Vote, Protection, KeywordToGrant, NameCard, CardFace, Sector, Sprocket }
pub enum ConfirmKind { OptionalTrigger(StackId), MayEffect, PayToPrevent, FlipCall,
                       StaticApplication, Bid(u32), PutOnTop(ObjId), KeepHand, Generic }

pub struct ActionRef(pub StackId);   // the in-progress cast/activation a sub-decision belongs to
```

---

## 6. Coverage matrix (the superset proof)

### 6.1 MTGA GRE `GREMessageType` request catalog → variant
(Catalog from `DECOMPILE_PLAN.md` §1. Every server→client `*Req` maps onto a variant.)

| GRE `*Req` | `DecisionRequest` variant |
|---|---|
| `ChooseStartingPlayerReq` | `ChooseStartingPlayer` |
| `MulliganReq` | `Mulligan` (+ `SelectCards{BottomForMulligan}`) |
| `ActionsAvailableReq` | `Priority` |
| `DeclareAttackersReq` | `DeclareAttackers` |
| `DeclareBlockersReq` | `DeclareBlockers` |
| `AssignDamageReq` | `AssignCombatDamage` |
| `OrderCombatDamageReq` | `OrderObjects{BlockersOf}` / `AssignCombatDamage` |
| `OrderReq` | `OrderObjects` |
| `SelectTargetsReq` | `ChooseTargets` |
| `SelectNReq` | `SelectCards` |
| `SelectNGroupReq` / `SelectFromGroupsReq` / `GroupReq` | `SelectFromGroups` |
| `SelectCountersReq` | `ChooseCounterType` |
| `SelectReplacementReq` | `ChooseReplacement` |
| `SearchReq` / `SearchFromGroupsReq` | `SelectCards{Search}` / `SelectFromGroups` |
| `DistributionReq` | `Distribute` |
| `PayCostsReq` | `PayCost` |
| `CastingTimeOptionsReq` | `CastingTimeOptions` (or decomposed: `ChooseModes`/`ChooseNumber`/`Confirm`) |
| `NumericInputReq` | `ChooseNumber` |
| `StringInputReq` | `ChooseOption{NameCard}` (constrained vocabulary) |
| `PromptReq` | `Confirm` / `ChooseOption` |
| `RevealHandReq` | `observe()` (push) / `SelectCards{Reveal}` if a choice is needed |
| `GatherReq` | `SelectCards` / `SelectFromGroups` |
| `AllowForceDraw` / `OptionalActionMessage` / `EdictalMessage` | `Confirm` |
| `IntermissionReq`/`TimeoutMessage`/`TimerStateMessage`/`UIMessage`/`PredictionResp` | not engine decisions — transport/UI; handled at the `MtgaClientAgent` layer, not in the enum |

### 6.2 Forge `PlayerController` (107 methods) → variant
(Grouped; every decision-making method maps. Pure-notification methods map to `observe`.)

| Forge method(s) | variant |
|---|---|
| `getAbilityToPlay`, `chooseSpellAbilityToPlay`, `playChosenSpellAbility`, `playSaFromPlayEffect`, `orderAndPlaySimultaneousSa` | `Priority` (+ `OrderObjects{SimultaneousSpellAbilities}`) |
| `chooseModeForAbility` | `ChooseModes` |
| `announceRequirements`, `chooseNumber`(×3), `chooseNumberForCostReduction`, `chooseNumberForKeywordCost` | `ChooseNumber` |
| `chooseOptionalCosts` | `CastingTimeOptions` |
| `chooseTargetsFor`, `chooseNewTargetsFor`, `chooseTarget`, `chooseSingleEntityForEffect`, `chooseEntitiesForEffect` | `ChooseTargets` |
| `divideShield`, divide-damage | `Distribute` |
| `payManaCost`(×2), `payCombatCost`, `payCostToPreventEffect`, `payCostDuringRoll`, `confirmPayment`, `specifyManaCombo`, `chooseManaFromPool`, `chooseCardsForConvokeOrImprovise`, `chooseCardsToDelve`, `orderCosts`, `helpPayForAssistSpell`, `choosePlayerToAssistPayment` | `PayCost` (+ `OrderObjects{CostSequence}`) |
| `declareAttackers`, `exertAttackers`, `enlistAttackers` | `DeclareAttackers` |
| `declareBlockers` | `DeclareBlockers` |
| `assignCombatDamage` | `AssignCombatDamage` |
| `orderBlockers`, `orderBlocker`, `orderAttackers`, `orderMoveToZoneList` | `OrderObjects` |
| `choosePermanentsToSacrifice`, `choosePermanentsToDestroy`, `chooseCardsForEffect`, `chooseCardsToDiscardFrom`, `chooseCardsToDiscardUnlessType`, `chooseCardsToDiscardToMaximumHandSize`, `chooseCardsToRevealFromHand`, `chooseSaToActivateFromOpeningHand`, `chooseSpellAbilitiesForEffect` | `SelectCards` |
| `chooseCardsForEffectMultiple` | `SelectFromGroups` |
| `arrangeForScry`, `arrangeForSurveil`, `willPutCardOnTop` | `ArrangeCards` (+ `Confirm{PutOnTop}`) |
| `chooseSingleReplacementEffect`, `confirmReplacementEffect`, `chooseSingleStaticAbility`, `confirmStaticApplication` | `ChooseReplacement` / `Confirm` |
| `chooseCounterType` | `ChooseCounterType` |
| `chooseSomeType`, `chooseSector`, `chooseSprocket`, `chooseProtectionType`, `chooseKeywordForPump`, `chooseCardName`, `chooseSingleCardFace`, `chooseSingleCardState`, `chooseSingleSpellForEffect`, `vote`, `chooseContraptionsToCrank` | `ChooseOption` |
| `chooseColor`, `chooseColorAllowColorless`, `chooseColors` | `ChooseColor` |
| `confirmAction`(×3), `confirmTrigger`, `confirmBidAction`, `chooseBinary`(×3), `chooseFlipResult`, `confirmMulliganScry`, `chooseCardsPile` | `Confirm` |
| `mulliganKeepHand`, `londonMulliganReturnCards` | `Mulligan` (+ `SelectCards{BottomForMulligan}`) |
| `chooseStartingPlayer`, `chooseStartingHand` | `ChooseStartingPlayer` / setup |
| `reveal`(×4), `notifyOfValue`, `revealAnte`, `revealAISkipCards`, `resetAtEndOfTurn` | `observe` (push-only) |
| `sideboard`, `chooseCardsYouWonToAddToDeck` | deck-construction, outside the in-game boundary (handle at match setup) |
| dice: `choosePDRollToIgnore`, `chooseRollToIgnore`, `chooseDiceToReroll`, `chooseRollToModify`, `chooseRollToSwap`, `chooseRollSwapValue` | `ChooseOption`/`SelectCards`/`ChooseNumber` (deferred — niche, CR 705/706) |

**Result:** every Forge decision method and every GRE `*Req` is covered. The enum is a strict
superset of both, as the laws require.

---

## 7. The Effect IR and the whiteboard `Action` (the data side of the boundary)

The agent interface is one half of WHITEBOARD_MODEL §2.6's "engine produces masked choice
points; backend answers." The other half is **what the engine is choosing among / about to
do** — the Effect IR (card behavior as data) and the `Action` whiteboard (staged mutations).
These are specified canonically in `WHITEBOARD_MODEL.md` §2.1/§2.3; this section pins the
Rust shape so task #4 can implement `crates/mtg-core/src/effects/` against it. **The core
engine must never `match` on card identity** (CLAUDE.md architecture law); all of the
following is *data interpreted by the effect runtime*.

### 7.1 `Action` — the whiteboard (staged, rewritable mutations)

The atomic, card-agnostic mutations the engine stages before committing (WHITEBOARD §2.1).
The replacement/prevention pass rewrites these; commit emits an `Event` per surviving action.

```rust
pub enum Action {
    Destroy     { obj: ObjId, source: Option<ObjId> },
    Sacrifice   { obj: ObjId, by: PlayerId },
    Damage      { target: Target, amount: u32, source: ObjId, kind: DamageKind },
    Draw        { player: PlayerId, count: u32 },
    Mill        { player: PlayerId, count: u32 },
    LoseLife    { player: PlayerId, amount: u32 },
    GainLife    { player: PlayerId, amount: u32 },
    MoveZone    { obj: ObjId, to: Zone, position: ZonePos, cause: MoveCause },
    TapUntap    { obj: ObjId, tap: bool },
    AddCounters { obj: ObjId, kind: CounterKind, n: i32 },
    CreateToken { spec: TokenSpec, controller: PlayerId },
    AttachTo    { attachment: ObjId, target: Target },
    Discard     { player: PlayerId, obj: ObjId },
    Exile       { obj: ObjId, source: Option<ObjId> },
    // … grows with the IR vocabulary (WHITEBOARD §5 "Granularity of Action: start medium")
}

pub struct Whiteboard {
    pub reason: WbReason,         // ResolveSpell(StackId) | CombatDamage | Cleanup-SBA | …
    pub actions: Vec<Action>,     // ordered; rules erase / replace / insert (CR 614.5/616)
    pub ctx: ResolutionCtx,       // controller, source, chosen modes/targets, X, …
}
```

### 7.2 The Effect IR — card abilities as data

The vocabulary an ability's effect tree is built from; resolving it *materializes* a
`Whiteboard` (WHITEBOARD §2.3). Leaves lower to `Action`s; interior nodes are control flow
and choice points (each choice point becomes a `DecisionRequest` via §3).

```rust
pub enum Effect {
    // ── leaves: lower to Action(s) ───────────────────────────────────────────────
    DealDamage { amount: ValueExpr, to: TargetSpec, kind: DamageKind },
    Draw       { who: PlayerRef, count: ValueExpr },
    Destroy    { what: TargetSpec },
    Sacrifice  { who: PlayerRef, what: SelectSpec },
    Mill       { who: PlayerRef, count: ValueExpr },
    GainLife   { who: PlayerRef, amount: ValueExpr },
    LoseLife   { who: PlayerRef, amount: ValueExpr },
    Pump       { what: TargetSpec, power: ValueExpr, toughness: ValueExpr, duration: Duration },
    AddMana    { who: PlayerRef, mana: ManaSpec },
    PutCounters{ what: TargetSpec, kind: CounterKind, n: ValueExpr },
    CreateToken{ spec: TokenSpec, count: ValueExpr },
    Counter    { what: TargetSpec },
    Search     { zone: Zone, who: PlayerRef, filter: CardFilter, min: u32, max: u32, then: ZoneDest },
    Tap        { what: TargetSpec, tap: bool },
    MoveZone   { what: TargetSpec, to: Zone },

    // ── composition / control flow (interior nodes) ──────────────────────────────
    Sequence  (Vec<Effect>),
    Optional   { prompt: ConfirmKind, body: Box<Effect> },        // "you may …"
    Modal      { modes: Vec<(ModeOption, Effect)>, min: u32, max: u32, allow_repeat: bool },
    Repeat     { count: ValueExpr, body: Box<Effect> },
    Conditional{ cond: Condition, then: Box<Effect>, otherwise: Option<Box<Effect>> },
    ForEach    { selector: SelectSpec, body: Box<Effect> },
    Distribute { total: ValueExpr, among: SelectSpec, min_each: u32, body: Box<Effect> },

    // ── escape hatch (WHITEBOARD §2.3): genuinely-unique cards supply Rust. ───────
    // The MTGA equivalent of "hand-authored CLIPS". Guarantees no card is ever
    // *impossible*, only *not-yet-done in pure IR*. Still card-agnostic at the core:
    // the core calls the fn pointer; it does not know which card it is.
    Native { name: &'static str, run: NativeFn },
}

/// Native effects receive a controlled context to read state, ask the agent (via the SAME
/// DecisionRequest boundary), and push Actions onto the current whiteboard. They may not
/// reach around the engine — the only state mutation is through `push_action`.
pub type NativeFn = fn(&mut EffectCtx) -> Result<(), EngineError>;
```

`ValueExpr`/`TargetSpec`/`SelectSpec`/`Condition`/`Duration`/`PlayerRef` are the
sub-vocabularies (counts that read game state, target selectors, filters, "until end of
turn", intervening-if conditions, etc.). They are deliberately small now and grow with the
card pool (WHITEBOARD §5 open question on `Action` granularity).

### 7.3 Ability kinds (the rule registry, WHITEBOARD §2.3)

```rust
pub enum Ability {
    /// instant/sorcery spell ability — resolves its Effect (CR 113.3a / 608).
    Spell      { effect: Effect },
    /// `Cost: Effect` (CR 602). Mana abilities are the no-stack subset (CR 605).
    Activated  { cost: Cost, effect: Effect, timing: Timing, restriction: Option<Restriction>, is_mana: bool },
    /// `When/Whenever/At [event] [, if cond] : Effect` (CR 603). Includes delayed/state triggers.
    Triggered  { event: EventPattern, condition: Option<Condition>, effect: Effect, intervening_if: bool },
    /// `[event/action pattern] -> rewrite` for the whiteboard rewrite pass (CR 614/615).
    Replacement{ pattern: ActionPattern, rewrite: Rewrite },
    /// contributes to a layer (CR 613) and/or sets a qualification (CR 613/611).
    Static     { contribution: StaticContribution },
}
```

These are the rule kinds the effect runtime registers and fires (the "CLIPS layer" of the
whiteboard model). The agent interface meets them at every choice point an `Effect`'s
`Modal`/`Optional`/`Distribute`/`Search`/target-selection introduces, and at every structural
choice (priority, combat, mulligan) the core engine raises directly.

---

## 8. How a decision is produced (engine-side contract)

Tying §3 to the engine loop (ENGINE_PLAN §4/§6, WHITEBOARD §2.2):

1. The engine reaches a point requiring a choice — either a **structural** one (priority,
   declare attackers, mulligan, ordering triggers) raised by the core machinery, or an
   **effect-driven** one (a `Modal`/target/`Distribute`/`Search` node while materializing a
   whiteboard).
2. The engine computes the **complete legal option set** (rules + timing + targeting + mana +
   restrictions/requirements) and builds the matching `DecisionRequest` variant. *This
   enumeration is the engine's masking responsibility — it is never delegated.*
3. The engine computes `view = state.view_for(actor)` and calls `agent.decide(&view, &req)`.
4. The engine **validates** the `DecisionResponse` is in-range (§4 contract) and applies it,
   continuing the agenda loop.
5. Side observations (reveals, opponents' chosen values, zone changes) are pushed to all
   seats via `observe`.

For the RL boundary specifically (GYM_PLAN §3/§4): the `Priority` request always includes
`Pass`; the engine **auto-passes** trivial priority windows (no legal non-pass action worth
surfacing) so the policy is consulted only at meaningful points; and the fixed global RL
action vocabulary derives its per-step mask directly from the enumerated options. None of
that changes the trait — it is policy on top of the same boundary.

### 8.1 Decision elision is an engine/Arena-profile concern — never per-agent

Whether a choice point is *raised at all* (auto-pass of trivial priority windows; eliding a
forced decision that has exactly one legal option) is decided by the **engine, governed by the
Arena profile** (ENGINE_PLAN §9) — *uniformly for every backend*. This is load-bearing for the
"interchangeable backends" guarantee and for differential-testing/replay (ENGINE_PLAN §8): all
backends must be consulted at the **same** decision points so the decision log replays
identically and a Forge-oracle diff compares like-for-like. A backend must **not** invent its
own elision (e.g. a GRE/web agent silently resolving a forced choice the engine never issued
while the RL agent sees a different call sequence) — that would desync the decision streams.

The permitted backend-local optimization is purely at the **transport** layer: when the engine
*does* issue a decision that happens to have a single legal option, an agent may answer it
locally (return the lone option) instead of doing a wire round-trip — because that returns the
*identical* response any other backend would. Elide-the-call = engine/profile (uniform);
skip-the-round-trip-for-a-lone-option = agent transport (fine, same answer). Keep the line
there.

---

## 9. Open questions / awaiting `decompile`

Resolve as the GRE schema (`mtga-re/schema/gre_schema.json`) lands; provisional choices are
marked above. I've messaged `decompile` for the field-level shapes:

- **Mulligan encoding.** Is London bottoming a separate GRE message or a field on
  `MulliganResp`? Provisional: keep/mulligan as `Mulligan`, bottoming as a follow-up
  `SelectCards{BottomForMulligan}`. Reconcile with `MulliganReq`/`MulliganResp`.
- **`NumericInputReq` constraints.** Does it carry min/max/forbidden, or just a free integer?
  This validates the `forbidden` field on `ChooseNumber` (WHITEBOARD §2.6 "forbidden X").
- **Target encoding in `SelectTargetsReq`.** Target-map vs criteria list; how multi-slot
  ("up to two target …") is represented → validates `TargetSlot`/`Pairs`.
- **`AssignDamageReq` + `OrderCombatDamageReq` split.** Is order a separate request or folded
  into assignment? → confirms `AssignCombatDamage` vs `OrderObjects{BlockersOf}` division.
- **`PayCostsReq` + `CastingTimeOptionsReq` granularity.** How much is batched vs sub-stepped
  (X, kicker, alternative/additional costs, Phyrexian/hybrid). `CastingTimeOptionType` enum
  values → fills `CastOption`.
- **`SelectN*`/`Group`/`Distribution`/`Replacement` field shapes** → finalize `SelectGroup`,
  `Distribute`, `ReplacementOption`.

When the schema arrives, the per-variant field details get pinned and the §6.1 table becomes
an exact field mapping; **the variant set itself is not expected to change** (it is already a
superset of the recovered catalog), so task #4 can proceed against this design now.

---

## 10. What implementation (task #4) will create

- `crates/mtg-core/src/agent.rs`: the `Agent` trait, `DecisionRequest`, `DecisionResponse`,
  `PlayerView` (+ the view types), and the supporting request types from §5. A `RandomAgent`
  reference impl (picks a uniform legal option) to prove the boundary end-to-end.
- `crates/mtg-core/src/effects/`: the `Effect` IR (`mod.rs`), `Action`/`Whiteboard`
  (`action.rs`), `Ability` kinds (`ability.rs`), the sub-vocabularies (`value.rs`,
  `target.rs`, `condition.rs`), and the `Native` hatch — per §7.
- Coordinate with `engine` on `lib.rs` module declarations and the shared id/`Zone`/`Phase`
  types to avoid a canonical-path or double-declaration clash (CLAUDE.md: one import path).
