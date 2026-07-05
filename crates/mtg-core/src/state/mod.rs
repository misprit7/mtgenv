//! Game state: `GameState`, `Player`, the seven zones, and `Object`s.
//! CR 108–112 (objects/permanents/tokens/spells/abilities), CR 400 (zones).
//!
//! Milestone 2: a minimal, cheaply-cloneable, serializable state sufficient for a
//! lands-only game (zones as `ObjId` vecs, an `ObjId`-keyed object arena, life/turn
//! pointers, active + priority player). The full characteristic/layer machinery
//! (`chars/`, CR 613) lands later; for now an object carries its printed/base
//! `Characteristics` and the computed view == the base.
//!
//! State stays index/`ObjId`-keyed (no pointer graphs) so `Clone` is a handful of `Vec`
//! copies — cheap for MCTS/vectorised envs (ENGINE_PLAN §7).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::basics::{CardType, Color, CounterBag, CounterKind, ManaCost, ManaPool, Phase, Status, Zone};
use crate::cards::{CardDb, CardDef};
use crate::effects::ability::{ActionPattern, FloatingRewrite, Keyword};
use crate::effects::action::{Action, DelayedTriggerEvent};
use crate::combat::CombatState;
use crate::ids::{ObjId, PlayerId, StackId, Timestamp};
use crate::rng::Rng;
use crate::subtypes::{LandType, Subtype, Supertype};
use crate::stack::{Stack, StackObject};

pub mod view;

/// The default starting life total in a two-player game (CR 103.4).
pub const STARTING_LIFE: i32 = 20;
/// The default opening-hand / maximum hand size (CR 103.5 / 514.1).
pub const DEFAULT_HAND_SIZE: usize = 7;

// `CardType` is shared vocabulary owned by `basics` (CR 300s); imported above. Reasoning
// about card *types* is structural Magic (the engine's job) — not card identity — so it
// doesn't violate the "never `match` on card identity" law (WHITEBOARD_MODEL §2 / CLAUDE.md).

/// The printed / base ("copiable", CR 707.2) characteristics of an object. The layer system
/// (`chars/`, CR 613) will later compute a derived cache from these; in milestone 2 the
/// computed characteristics *are* the base.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Characteristics {
    pub name: String,
    pub card_types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub supertypes: Vec<Supertype>,
    pub colors: Vec<Color>,
    pub mana_cost: Option<ManaCost>,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    /// Printed keyword abilities (CR 702). The layer system (`chars/`) seeds the computed
    /// keyword set from these, then layers grants/removes (layer 6) on top.
    pub keywords: Vec<Keyword>,
    /// Printed starting loyalty for a planeswalker (CR 306.5b) — it enters the battlefield with
    /// this many loyalty counters. `None` for non-planeswalkers.
    pub loyalty: Option<i32>,
    /// Oracle/printing id for embedding-table lookups (RL) & rendering; 0 = unset.
    pub grp_id: u32,
}

impl Characteristics {
    pub fn has_type(&self, t: CardType) -> bool {
        self.card_types.contains(&t)
    }
    pub fn is_land(&self) -> bool {
        self.has_type(CardType::Land)
    }
    pub fn is_creature(&self) -> bool {
        self.has_type(CardType::Creature)
    }
    /// Whether a spell of these characteristics resolves into a permanent (CR 110.4 / 608.3).
    pub fn is_permanent(&self) -> bool {
        self.card_types.iter().any(|t| t.is_permanent())
    }
    /// Mana value (CR 202.3): generic + the sum of all colored pips + one per two-colour hybrid pip
    /// + the generic-side amount of each monocolour hybrid pip (`{2/R}` counts 2, CR 202.3g).
    pub fn mana_value(&self) -> u32 {
        match &self.mana_cost {
            Some(c) => {
                c.generic
                    + c.colored.values().copied().sum::<u32>()
                    + c.hybrid.len() as u32
                    + c.mono_hybrid.iter().map(|&(n, _)| n).sum::<u32>()
            }
            None => 0,
        }
    }

    /// A basic land card. Its single basic land-type subtype is derived from the name (CR 305.6);
    /// the engine reads that subtype to grant the intrinsic `{T}: Add <colour>` mana ability.
    pub fn basic_land(name: &str) -> Self {
        let land_type: LandType = name.parse().expect("basic land name must be a basic land type");
        Characteristics {
            name: name.to_string(),
            card_types: vec![CardType::Land],
            supertypes: vec![Supertype::Basic],
            subtypes: vec![land_type.into()],
            ..Default::default()
        }
    }
}

/// A game object (CR 109.1) with a stable identity. Milestone 2 keeps battlefield status,
/// counters and marked damage so the structure is ready for combat/SBAs even though a
/// lands-only game exercises few of them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: ObjId,
    pub owner: PlayerId,
    /// Only meaningful on the stack / battlefield (CR 109.4); defaults to owner elsewhere.
    pub controller: PlayerId,
    pub zone: Zone,
    pub chars: Characteristics,
    pub status: Status,
    pub counters: CounterBag,
    pub damage_marked: u32,
    /// Set when this creature has been dealt damage by a deathtouch source this turn; the SBA
    /// (CR 704.5h) then destroys it regardless of amount. Cleared at cleanup with marked damage.
    pub dealt_deathtouch: bool,
    /// Summoning sickness (CR 302.6): can't attack / use `{T}` until controlled since the
    /// start of its controller's most recent turn (unless it has haste).
    pub summoning_sick: bool,
    /// Timestamp for the layer system (CR 613.7): assigned when the object enters the
    /// battlefield; orders continuous effects within a sublayer.
    pub timestamp: Timestamp,
    /// The permanent this object is attached to (CR 701.3) — set for Auras and Equipment.
    /// `None` for unattached permanents and anything off the battlefield. The continuous
    /// effects an attached permanent grants read this via `CardFilter::AttachedHost`.
    pub attached_to: Option<ObjId>,
    /// Set when an ability with the once-per-turn restriction (a planeswalker loyalty ability,
    /// CR 606.3) has been activated from this permanent this turn; reset at the start of each
    /// turn and on any zone change. Blocks a second loyalty activation.
    pub used_once_per_turn: bool,
    /// The total mana spent to cast this object (CR 601.2f–h, incl. `{X}`), recorded by `cast_spell`
    /// while it's on the stack and read by an enters-with-counters replacement as it resolves onto
    /// the battlefield (Dyadrine). Reset to 0 on every zone change (a fresh object, CR 400.7), so a
    /// permanent put onto the battlefield without being cast reads 0.
    pub mana_spent: u32,
    /// The number of **distinct colours of mana spent** to cast this object (CR 702.75 Converge),
    /// recorded by `cast_spell` alongside `mana_spent`. Reset to 0 on every zone change (CR 400.7).
    #[serde(default)]
    pub colors_spent: u32,
    /// The value chosen for `{X}` when this object was cast (CR 107.3 / 601.2b), recorded by
    /// `cast_spell` alongside `mana_spent`. `None` if the cost had no `{X}`. Read by
    /// `ValueExpr::XOfTriggeringSpell` — "look at the top X cards" where X is the triggering spell's
    /// {X} (Geometer's Arthropod). Reset on every zone change (a fresh object, CR 400.7).
    #[serde(default)]
    pub cast_x: Option<u32>,
    /// While this card is in exile, the permanent that exiled it (Keen-Eyed Curator's "cards exiled
    /// **with** this creature") — set by `Action::Exile` from the exiling source, `None` otherwise.
    /// Reset on every zone change (a card leaving exile drops the link, CR 400.7).
    pub exiled_with: Option<ObjId>,
    /// Set while this object is a spell cast for its **warp** cost (CR 702.x) — so when it resolves
    /// onto the battlefield the engine arms the "exile at the next end step" delayed trigger. Reset
    /// on every zone change.
    pub warp_cast: bool,
    /// Set while this object is a spell cast for its **flashback** cost (CR 702.34) — so when it
    /// leaves the stack the engine exiles it instead of putting it in the graveyard. Reset on every
    /// zone change.
    pub flashback_cast: bool,
    /// Set on a card warp-exiled at its end step (CR 702.x) — it may be cast from exile on a later
    /// turn (for its normal cost). Reset on any zone change (cast it, or it leaves exile).
    pub castable_from_exile: bool,
    /// The last turn number on which an **impulse-exiled** card may be played (inclusive) — "you may
    /// play it until end of turn / your next turn" (SoS impulse-play). `None` = no turn limit (a
    /// warp-exiled card, playable any later turn). Read alongside `castable_from_exile` at the offer;
    /// reset on any zone change (CR 400.7).
    #[serde(default)]
    pub play_until_turn: Option<u32>,
    /// Set when one or more counters were put on this permanent **this turn** (any counter kind, via
    /// `Action::AddCounters` with positive `n`). Reset at the start of each turn and on any zone change
    /// (a fresh object, CR 400.7). Read by the SoS Quandrix "if you put a counter on this creature this
    /// turn" gate (Fractal Tender).
    #[serde(default)]
    pub counter_added_this_turn: bool,
    /// Set on a **copy of a spell** put on the stack (CR 707, e.g. a Paradigm free-cast copy). A copy
    /// isn't a card, so when it leaves the stack it **ceases to exist** (CR 707.10a) instead of going
    /// to a graveyard/exile — `resolve_top`/`interpret_counter` route it through `cease_to_exist`.
    /// Reset on any zone change (a copy should never reach a normal zone, but keep the invariant).
    #[serde(default)]
    pub is_copy: bool,
    /// Set on a **prepared** permanent (SoS "Prepare" DFCs, CR 711-adjacent — modeled as a spell-copy
    /// consumer, not a transform). While a creature with an [`crate::effects::ability::Ability::Prepare`]
    /// marker is prepared, its controller may cast a *copy* of its back-face spell (a paid
    /// [`crate::agent::PlayableAction::CastPrepared`]); doing so unprepares it. Set by
    /// [`crate::effects::Effect::BecomePrepared`] (via [`crate::effects::action::Action::SetPrepared`]),
    /// which every "becomes prepared" clause (enters-prepared / at-first-main / on-attack / activated /
    /// landfall …) lowers to. Reset on any zone change (a fresh object identity, CR 400.7).
    #[serde(default)]
    pub prepared: bool,
}

impl Object {
    /// Net `+1/+1` minus `-1/-1` counters (the only P/T-modifying counters; CR 122.1a).
    pub(crate) fn counter_pt_delta(&self) -> i32 {
        self.counters.get(&CounterKind::PlusOnePlusOne) as i32
            - self.counters.get(&CounterKind::MinusOneMinusOne) as i32
    }
    /// Effective power = base power + counter delta. (Trivial pre-layer-system P/T, CR 613
    /// layer 7c; the full layer system is milestone 5.)
    pub fn effective_power(&self) -> i32 {
        self.chars.power.unwrap_or(0) + self.counter_pt_delta()
    }
    /// Effective toughness = base toughness + counter delta.
    pub fn effective_toughness(&self) -> i32 {
        self.chars.toughness.unwrap_or(0) + self.counter_pt_delta()
    }
}

/// One seat. Zones it owns are `ObjId` vectors into [`GameState::objects`]. Library order is
/// significant: **the top of the library is the last element** (so a draw is a `pop`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub life: i32,
    pub poison: u32,
    pub library: Vec<ObjId>,
    pub hand: Vec<ObjId>,
    pub battlefield: Vec<ObjId>,
    pub graveyard: Vec<ObjId>,
    pub exile: Vec<ObjId>,
    /// Emblems this player owns (CR 114 / 408). The command zone is modeled per-player (each emblem's
    /// owner/controller is the player who got it); emblems sit here permanently and are untouchable by
    /// battlefield removal/SBAs. Empty for players with no emblems.
    #[serde(default)]
    pub command: Vec<ObjId>,
    pub mana_pool: ManaPool,
    pub counters: CounterBag,
    /// Lands played this turn (CR 116.2a / 505.6b: one per turn by default).
    pub lands_played_this_turn: u32,
    /// Total life gained this turn (CR 118.9) — reset at the start of each turn, incremented by each
    /// `GainLife`. Read by the SoS "Infusion — if you gained life this turn …" condition.
    #[serde(default)]
    pub life_gained_this_turn: u32,
    /// How many separate life-gain **events** this player has had this turn (CR 119.3) — reset each
    /// turn, incremented once per positive `LifeChanged`. Distinct from `life_gained_this_turn` (which
    /// sums amounts): this counts occurrences, for "whenever you gain life for the first time each turn"
    /// (Leech Collector), gated as `life_gain_events_this_turn == 1` at trigger-queue time.
    #[serde(default)]
    pub life_gain_events_this_turn: u32,
    /// How many cards have left this player's graveyard this turn — reset each turn, incremented when
    /// an object moves out of the graveyard. Read by the SoS Lorehold "if a card left your graveyard
    /// this turn …" condition.
    #[serde(default)]
    pub cards_left_graveyard_this_turn: u32,
    /// How many creatures died under this player's control this turn (CR 700.4) — reset each turn,
    /// incremented in the creature-death SBA. Read by "if a creature died under your control this
    /// turn" (Essenceknit Scholar).
    #[serde(default)]
    pub creatures_died_this_turn: u32,
    /// How many cards this player has drawn this turn (CR 120) — reset each turn, incremented in
    /// `draw`. Read by the SoS Quandrix "X = the number of cards you've drawn this turn" value
    /// (Fractal Anomaly).
    #[serde(default)]
    pub cards_drawn_this_turn: u32,
    /// How many instant/sorcery spells this player has cast this turn — reset each turn, incremented
    /// in `cast_spell`. Read by "if you've cast an instant or sorcery spell this turn" (Potioner's
    /// Trove) via `Condition::CastInstantOrSorceryThisTurn`.
    #[serde(default)]
    pub instants_sorceries_cast_this_turn: u32,
    /// How many spells of **any** type this player has cast this turn — reset each turn, incremented in
    /// `cast_spell`. Read by "whenever you cast your Nth spell each turn" (Emeritus of Conflict) via a
    /// `ValueExpr::SpellsCastThisTurn` gate.
    #[serde(default)]
    pub spells_cast_this_turn: u32,
    pub hand_size_limit: usize,
    pub has_lost: bool,
    /// Set when a draw is attempted from an empty library; the SBA (CR 704.5b) reads it on
    /// the next check, then the player loses.
    pub drew_from_empty: bool,
}

impl Player {
    fn new(id: PlayerId) -> Self {
        Player {
            id,
            life: STARTING_LIFE,
            poison: 0,
            library: Vec::new(),
            hand: Vec::new(),
            battlefield: Vec::new(),
            graveyard: Vec::new(),
            exile: Vec::new(),
            command: Vec::new(),
            mana_pool: ManaPool::default(),
            counters: CounterBag::default(),
            lands_played_this_turn: 0,
            life_gained_this_turn: 0,
            life_gain_events_this_turn: 0,
            cards_left_graveyard_this_turn: 0,
            creatures_died_this_turn: 0,
            cards_drawn_this_turn: 0,
            instants_sorceries_cast_this_turn: 0,
            spells_cast_this_turn: 0,
            hand_size_limit: DEFAULT_HAND_SIZE,
            has_lost: false,
            drew_from_empty: false,
        }
    }

    /// The `ObjId`s in one of this player's zones (empty for `Stack`, which is global).
    pub fn zone_ids(&self, zone: Zone) -> &[ObjId] {
        match zone {
            Zone::Library => &self.library,
            Zone::Hand => &self.hand,
            Zone::Battlefield => &self.battlefield,
            Zone::Graveyard => &self.graveyard,
            Zone::Exile => &self.exile,
            Zone::Command => &self.command,
            Zone::Stack => &[],
        }
    }

    /// The owned `ObjId` vector for a per-player zone (everything except the global `Stack`).
    fn zone_vec_mut(&mut self, zone: Zone) -> Option<&mut Vec<ObjId>> {
        match zone {
            Zone::Library => Some(&mut self.library),
            Zone::Hand => Some(&mut self.hand),
            Zone::Battlefield => Some(&mut self.battlefield),
            Zone::Graveyard => Some(&mut self.graveyard),
            Zone::Exile => Some(&mut self.exile),
            Zone::Command => Some(&mut self.command),
            Zone::Stack => None,
        }
    }
}

/// Last-known information for a permanent that left the battlefield (CR 603.10a): its computed
/// characteristics and controller as they last existed on the battlefield. Read by dies/LTB
/// triggers (whose object is now in another zone, where its controller/counters/P-T differ).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lki {
    pub chars: crate::chars::ComputedChars,
    pub controller: PlayerId,
}

/// An armed delayed triggered ability (CR 603.7) waiting on its watched object. Carries the
/// concrete [`Action`]s to run when it resolves (not an `Effect` tree), so it stays serializable
/// and card-agnostic — the engine never matches on card identity to fire it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DelayedTrigger {
    pub id: u64,
    /// The object whose leaving the battlefield arms this trigger.
    pub watching: ObjId,
    pub event: DelayedTriggerEvent,
    /// Who controls the delayed ability (puts it on the stack, CR 603.7d).
    pub controller: PlayerId,
    /// The object that created it (for LKI / the ability's source).
    pub source: Option<ObjId>,
    /// What to do when it resolves — e.g. "return [watching] to the battlefield tapped".
    pub actions: Vec<Action>,
}

/// A **floating replacement effect** (CR 614) created at resolution and scoped to a single object for
/// a duration — the general container for "if [scope] would [pattern], [rewrite] instead" riders
/// (Wilt in the Heat: "if that creature would die this turn, exile it instead"). Kept general
/// (pattern + rewrite + scope + duration + one-shot) so future "would deal damage"-style floaters ride
/// the same rails. Consulted by the same rewrite pass as printed statics (`GameState.floating_replacements`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FloatingReplacement {
    /// The specific object this rider watches (CR 400.7: it is a *new* object if it changes zones, so
    /// the rider is invalidated when `scope` leaves the battlefield — it never chases the object back).
    pub scope: ObjId,
    /// What action, on `scope`, this replaces (e.g. `WouldDie`).
    pub pattern: ActionPattern,
    /// How the matched action is rewritten (serde-safe subset of `Rewrite`).
    pub rewrite: FloatingRewrite,
    /// Last turn (inclusive) this rider is active — removed at the start of a later turn (CR 514
    /// cleanup / "this turn" durations).
    pub until_turn: u32,
    /// Removed the first time it applies (CR 614.5-style single use) — a one-shot "exile it instead".
    pub one_shot: bool,
}

/// The whole game (CR 100s). Cheaply cloneable & serializable for snapshots/replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub objects: BTreeMap<ObjId, Object>,
    pub players: Vec<Player>,
    pub turn_number: u32,
    pub active_player: PlayerId,
    pub priority_player: Option<PlayerId>,
    pub phase: Phase,
    pub stack: Stack,
    /// The player who took the first turn (CR 103.8a: they skip their first draw step).
    pub starting_player: PlayerId,
    /// Triggered abilities waiting to be put on the stack the next time a player would get
    /// priority (CR 603.3, APNAP-ordered). Empty until the effect runtime arrives (M4); the
    /// agenda loop already drains it so the wiring is correct from day one.
    pub pending_triggers: Vec<StackObject>,
    /// For a "whenever you cast …" triggered ability (CR 603.2) queued off a spell cast: maps that
    /// trigger's [`StackId`] to the **triggering spell's** card [`ObjId`], so the ability can read
    /// the spell's mana-spent at resolution (SoS "Opus"). Cleared per entry when the trigger
    /// resolves; empty otherwise.
    #[serde(default)]
    pub trigger_source_spell: BTreeMap<StackId, ObjId>,
    /// For a `BecomesTargeted` trigger (CR 603.2) queued off a spell/ability targeting a permanent:
    /// maps that trigger's [`StackId`] to the **targeting spell/ability's** [`StackId`], so a Ward
    /// soft-counter (CR 702.21) can counter "that spell or ability" at resolution. Cleared per entry
    /// when the trigger resolves; empty otherwise.
    #[serde(default)]
    pub trigger_targeting_source: BTreeMap<StackId, StackId>,
    /// Combat state during a combat phase (CR 506–511); `None` outside combat.
    pub combat: Option<CombatState>,
    /// Continuous effects created by resolution (CR 611) that aren't printed `Ability::Static` —
    /// "until end of turn" pumps, animations (Earthbend), etc. Folded into the layer system
    /// (`chars::compute`) alongside printed statics. Real game state (serialized for snapshot/replay).
    pub continuous_effects: Vec<crate::chars::ContinuousEffect>,
    /// **Floating replacement effects** (CR 614) created by resolution and scoped to a specific object
    /// for a duration — e.g. "if that creature would die this turn, exile it instead" (Wilt in the
    /// Heat). Consulted by the same rewrite pass as permanents' printed `Ability::Replacement`
    /// statics, so CR 616.1 ordering keeps working. Real game state (serialized), so the rewrite is a
    /// serde-safe [`FloatingRewrite`], not a [`crate::effects::ability::Rewrite`] (which carries an
    /// `Effect`). Auto-invalidated when the scoped object changes zones (CR 400.7) and at turn expiry.
    #[serde(default)]
    pub floating_replacements: Vec<FloatingReplacement>,
    /// Armed delayed triggered abilities (CR 603.7): "when [watching] dies/is exiled, do …". The
    /// engine fires (and consumes) one when its watched object leaves the battlefield. Real state.
    pub delayed_triggers: Vec<DelayedTrigger>,
    /// Last-known information (CR 603.10a / 608.2h): a snapshot of each permanent's computed
    /// characteristics + controller, taken as it **left the battlefield**. Leaves-the-battlefield
    /// triggers (dies-triggers) evaluate their filter/value against this — a creature in the
    /// graveyard has no controller and its base P/T (no counters/effects), so the live object is the
    /// wrong source. Captured in `move_object`, keyed by the object's stable `ObjId`; overwritten on
    /// each battlefield-leave. Load-bearing for every present and future dies/LTB ability.
    #[serde(default)]
    pub last_known: BTreeMap<ObjId, Lki>,
    pub game_over: bool,
    pub winner: Option<PlayerId>,
    /// Why the game ended (the loss reason of the first player to lose), or `None` while the
    /// game is in progress or if it ended by draw / turn-cap.
    pub end_reason: Option<crate::sba::LossReason>,
    pub rng: Rng,
    /// The card definitions (abilities = Effect IR) for cards in this game. Card *data*, not
    /// snapshot state: shared via `Arc` (clone is O(1)) and **not serialized** (a snapshot
    /// re-attaches the db on load). Looked up by object `grp_id`.
    #[serde(skip)]
    pub card_db: Arc<CardDb>,
    next_obj: u64,
    next_stack: u64,
    next_timestamp: u64,
    next_effect_id: u64,
    next_delayed_id: u64,
    /// Layer-system cache (CR 613): computed characteristics per battlefield object, rebuilt
    /// on the dirty signal. Derived data — not serialized; recomputed on demand after load.
    #[serde(skip)]
    chars_cache: BTreeMap<ObjId, crate::chars::ComputedChars>,
    /// Set when continuous-effect inputs change (zone/counter/ability/timestamp); the agenda's
    /// recompute step rebuilds the cache and clears it (WHITEBOARD_MODEL §2.4).
    #[serde(skip)]
    chars_dirty: bool,
}

impl GameState {
    /// A fresh game with `num_players` seats, all libraries empty. The caller populates
    /// libraries (e.g. with [`GameState::add_card`]) then the engine deals opening hands.
    pub fn new(num_players: usize, seed: u64) -> Self {
        let players = (0..num_players)
            .map(|i| Player::new(PlayerId(i as u32)))
            .collect();
        GameState {
            objects: BTreeMap::new(),
            players,
            turn_number: 1,
            active_player: PlayerId(0),
            priority_player: None,
            phase: Phase::Untap,
            stack: Stack::default(),
            starting_player: PlayerId(0),
            pending_triggers: Vec::new(),
            trigger_source_spell: BTreeMap::new(),
            trigger_targeting_source: BTreeMap::new(),
            combat: None,
            continuous_effects: Vec::new(),
            floating_replacements: Vec::new(),
            delayed_triggers: Vec::new(),
            last_known: BTreeMap::new(),
            game_over: false,
            winner: None,
            end_reason: None,
            rng: Rng::new(seed),
            card_db: Arc::new(CardDb::default()),
            next_obj: 1,
            next_stack: 1,
            next_timestamp: 1,
            next_effect_id: 1,
            next_delayed_id: 1,
            chars_cache: BTreeMap::new(),
            chars_dirty: true,
        }
    }

    /// Attach the card-definition registry (call once at game setup).
    pub fn set_card_db(&mut self, db: Arc<CardDb>) {
        self.card_db = db;
    }
    /// The card-definition registry (shared clone of the `Arc`).
    pub fn card_db(&self) -> Arc<CardDb> {
        Arc::clone(&self.card_db)
    }
    /// The definition of an object, looked up by its `grp_id`.
    pub fn def_of(&self, id: ObjId) -> Option<&CardDef> {
        let grp = self.objects.get(&id)?.chars.grp_id;
        self.card_db.get(grp)
    }
    /// A registered card definition by `grp_id` directly — for defs with no live object yet (a
    /// prepared creature's copy-only back-face spell, minted on demand; SoS Prepare).
    pub fn def_by_grp(&self, grp: u32) -> Option<&CardDef> {
        self.card_db.get(grp)
    }

    pub fn player(&self, p: PlayerId) -> &Player {
        &self.players[p.0 as usize]
    }
    pub fn player_mut(&mut self, p: PlayerId) -> &mut Player {
        &mut self.players[p.0 as usize]
    }
    pub fn object(&self, id: ObjId) -> &Object {
        &self.objects[&id]
    }

    /// Mint a fresh, never-reused object id.
    pub fn mint_obj(&mut self) -> ObjId {
        let id = ObjId(self.next_obj);
        self.next_obj += 1;
        id
    }
    // Used by casting to put spells/abilities on the stack (milestone 3) and by stack tests.
    #[allow(dead_code)]
    pub(crate) fn mint_stack(&mut self) -> crate::ids::StackId {
        let id = crate::ids::StackId(self.next_stack);
        self.next_stack += 1;
        id
    }
    /// A fresh layer-system timestamp (CR 613.7), assigned when an object enters the
    /// battlefield.
    fn mint_timestamp(&mut self) -> Timestamp {
        let t = Timestamp(self.next_timestamp);
        self.next_timestamp += 1;
        t
    }

    /// Mark the continuous-effect cache stale (CR 613.5 dirty signal).
    pub(crate) fn mark_chars_dirty(&mut self) {
        self.chars_dirty = true;
    }
    pub(crate) fn chars_is_dirty(&self) -> bool {
        self.chars_dirty
    }
    /// Computed characteristics for a battlefield object (CR 613). Reads the cache when fresh,
    /// else computes on demand — so the result is always correct even between recomputes.
    pub fn computed(&self, id: ObjId) -> crate::chars::ComputedChars {
        if !self.chars_dirty {
            if let Some(c) = self.chars_cache.get(&id) {
                return c.clone();
            }
        }
        crate::chars::compute(self, id)
    }
    /// Rebuild the layer-system cache for every battlefield object and clear the dirty flag
    /// (the agenda's recompute step, WHITEBOARD_MODEL §2.4). Sweeps dead floating continuous
    /// effects first so the cache never reflects an effect whose objects have all left.
    pub(crate) fn recompute_continuous(&mut self) {
        self.expire_continuous_effects();
        let ids: Vec<ObjId> = self
            .players
            .iter()
            .flat_map(|p| p.battlefield.iter().copied())
            .collect();
        let mut cache = BTreeMap::new();
        for id in ids {
            cache.insert(id, crate::chars::compute(self, id));
        }
        self.chars_cache = cache;
        self.chars_dirty = false;
    }

    /// Register a continuous effect created by resolution (CR 611) over a fixed set of objects.
    /// Mints a fresh layer timestamp (CR 613.7d — a resolution-created effect orders after every
    /// effect that already existed) and marks the layer cache dirty. Returns the effect's id (for
    /// later targeted removal). See [`crate::chars::ContinuousEffect`].
    pub(crate) fn add_continuous_effect(
        &mut self,
        source: Option<ObjId>,
        controller: PlayerId,
        affected: Vec<ObjId>,
        contributions: Vec<crate::effects::ability::StaticContribution>,
        duration: crate::effects::condition::Duration,
    ) -> u64 {
        let id = self.next_effect_id;
        self.next_effect_id += 1;
        let timestamp = self.mint_timestamp();
        let start_turn = self.turn_number;
        self.continuous_effects.push(crate::chars::ContinuousEffect {
            id,
            timestamp,
            source,
            controller,
            affected,
            contributions,
            duration,
            start_turn,
        });
        self.mark_chars_dirty();
        id
    }

    /// Arm a delayed triggered ability (CR 603.7). Returns its id. The engine fires and consumes
    /// it when `watching` leaves the battlefield matching `event`. See [`DelayedTrigger`].
    pub(crate) fn register_delayed_trigger(
        &mut self,
        watching: ObjId,
        event: DelayedTriggerEvent,
        controller: PlayerId,
        source: Option<ObjId>,
        actions: Vec<Action>,
    ) -> u64 {
        let id = self.next_delayed_id;
        self.next_delayed_id += 1;
        self.delayed_triggers.push(DelayedTrigger {
            id,
            watching,
            event,
            controller,
            source,
            actions,
        });
        id
    }

    /// End "until end of turn" / "this turn" continuous effects at cleanup (CR 514.2) — e.g. a
    /// +X/+0 pump wearing off. Marks the layer cache dirty if any were removed.
    pub(crate) fn end_of_turn_continuous_cleanup(&mut self) {
        use crate::effects::condition::Duration;
        let before = self.continuous_effects.len();
        self.continuous_effects
            .retain(|ce| !matches!(ce.duration, Duration::UntilEndOfTurn | Duration::ThisTurn));
        if self.continuous_effects.len() != before {
            self.mark_chars_dirty();
        }
    }

    /// Drop floating continuous effects that no longer apply to anything: an effect all of whose
    /// affected objects have left the battlefield is moot (CR 611.2c/400.7 — the object it was
    /// pinned to is a different object now), so it's garbage-collected to keep the list bounded.
    /// Duration-based expiry (cleanup / your-next-turn) is handled by the turn machinery.
    pub(crate) fn expire_continuous_effects(&mut self) {
        let on_bf: std::collections::BTreeSet<ObjId> = self
            .players
            .iter()
            .flat_map(|p| p.battlefield.iter().copied())
            .collect();
        let before = self.continuous_effects.len();
        self.continuous_effects
            .retain(|ce| ce.affected.iter().any(|id| on_bf.contains(id)));
        if self.continuous_effects.len() != before {
            self.chars_dirty = true;
        }
    }

    /// Create an object owned by `owner` and place it (appended) into one of that player's
    /// zones. Returns its id. Used to build decks.
    pub fn add_card(&mut self, owner: PlayerId, chars: Characteristics, zone: Zone) -> ObjId {
        let id = self.mint_obj();
        let timestamp = if zone == Zone::Battlefield {
            self.mint_timestamp()
        } else {
            Timestamp(0)
        };
        let obj = Object {
            id,
            owner,
            controller: owner,
            zone,
            chars,
            status: Status::default(),
            counters: CounterBag::default(),
            damage_marked: 0,
            dealt_deathtouch: false,
            summoning_sick: false,
            timestamp,
            attached_to: None,
            used_once_per_turn: false,
            mana_spent: 0,
            colors_spent: 0,
            cast_x: None,
            exiled_with: None,
            warp_cast: false,
            flashback_cast: false,
            castable_from_exile: false,
            play_until_turn: None,
            counter_added_this_turn: false,
            is_copy: false,
            prepared: false,
        };
        self.objects.insert(id, obj);
        if let Some(v) = self.player_mut(owner).zone_vec_mut(zone) {
            v.push(id);
        }
        if zone == Zone::Battlefield {
            self.enter_with_loyalty(id);
            self.mark_chars_dirty();
        }
        id
    }

    /// CR 306.5b: a planeswalker enters the battlefield with loyalty counters equal to its
    /// printed loyalty. (Loyalty-modifying replacements like Doubling Season are future.)
    fn enter_with_loyalty(&mut self, id: ObjId) {
        if let Some(o) = self.objects.get_mut(&id) {
            if let Some(loy) = o.chars.loyalty {
                o.counters.counts.insert(CounterKind::Loyalty, loy.max(0) as u32);
            }
        }
    }

    /// Move an object between *per-player* zones, keeping the arena and the zone vectors in
    /// sync. `to_owner` controls which player's zone it lands in (e.g. a spell goes to its
    /// **owner's** graveyard, CR 608.2n; a played land enters the battlefield under the
    /// player who played it). Returns false if the object wasn't found.
    ///
    /// NOTE (CR 400.7): a zone change generally mints a *new* object identity. Milestone 2
    /// reuses the id (lands-only carries no counters/continuous effects, so nothing depends
    /// on the new-object rule yet); this is revisited when LKI/counters/effects make it
    /// observable.
    /// CR 707.10a: a copy of a spell that leaves the stack **ceases to exist** — it isn't a card, so
    /// it goes to no zone. A stack object lives only in `objects` (its `zone` is `Zone::Stack`, never
    /// in a player zone vec — see `zone_vec_mut`), so removing it from the arena fully deletes it.
    pub(crate) fn cease_to_exist(&mut self, id: ObjId) {
        self.objects.remove(&id);
    }

    pub(crate) fn move_object(&mut self, id: ObjId, to: Zone, to_owner: PlayerId) -> bool {
        let (from_zone, from_owner, from_controller) = match self.objects.get(&id) {
            Some(o) => (o.zone, o.owner, o.controller),
            None => return false,
        };
        // Which player's zone-vec the object currently sits in: a battlefield/stack permanent lives in
        // its CONTROLLER's vec (CR 109.4 — `move_object` pushed it to `to_owner` and set that as the
        // controller), whereas in any other zone it lives in its OWNER's vec. These coincide whenever
        // control == owner (the common case), but a control-override reanimation (Reanimate) puts an
        // opponent-owned card under your control — so source removal must key on the controller for
        // battlefield sources, else the object would be searched for in the wrong player's vec.
        let from_holder = if from_zone == Zone::Battlefield || from_zone == Zone::Stack {
            from_controller
        } else {
            from_owner
        };
        // Capture last-known information (CR 603.10a) as the permanent LEAVES the battlefield —
        // before its controller/counters/damage are reset below — so dies/LTB triggers see how it
        // last existed. Snapshot the computed chars now (immutable borrow ends before we mutate).
        if from_zone == Zone::Battlefield && to != Zone::Battlefield {
            let chars = self.computed(id);
            let controller = self.objects.get(&id).map(|o| o.controller).unwrap_or(from_owner);
            self.last_known.insert(id, Lki { chars, controller });
            // CR 400.7: an object leaving the battlefield becomes a *new* object; floating replacement
            // riders scoped to it (e.g. "if that creature would die this turn, exile it instead") are
            // invalidated so they never chase the object back if it returns. (A death that was itself
            // redirected to exile already consumed its one-shot rider in the rewrite pass.)
            self.floating_replacements.retain(|f| f.scope != id);
        }
        // Remove from the source zone vector (keyed by the holder computed above, not always owner).
        if let Some(v) = self.player_mut(from_holder).zone_vec_mut(from_zone) {
            if let Some(pos) = v.iter().position(|&x| x == id) {
                v.remove(pos);
            }
        }
        // "A card left your graveyard this turn" (SoS Lorehold): count departures from the graveyard.
        if from_zone == Zone::Graveyard && to != Zone::Graveyard {
            self.player_mut(from_owner).cards_left_graveyard_this_turn += 1;
        }
        // A permanent entering the battlefield gets a fresh layer-system timestamp (613.7d).
        let new_ts = if to == Zone::Battlefield {
            Some(self.mint_timestamp())
        } else {
            None
        };
        // Update the object, then append to the destination zone vector.
        if let Some(o) = self.objects.get_mut(&id) {
            o.zone = to;
            // A permanent enters untapped/unflipped/face-up/phased-in (CR 110.5b); status,
            // counters and marked damage exist only on the battlefield (CR 110.5d), so reset
            // them on every zone change either way.
            o.status = Status::default();
            o.counters = CounterBag::default();
            o.damage_marked = 0;
            o.dealt_deathtouch = false;
            o.used_once_per_turn = false; // a fresh object identity (CR 400.7)
            o.mana_spent = 0; // re-recorded only by a fresh cast (CR 400.7)
            o.colors_spent = 0; // re-recorded only by a fresh cast (CR 400.7)
            o.cast_x = None; // re-recorded only by a fresh cast (CR 400.7)
            o.exiled_with = None; // the exile-association is dropped on any zone change (400.7)
            o.warp_cast = false; // a fresh object identity (CR 400.7)
            o.flashback_cast = false; // a fresh object identity (CR 400.7)
            o.castable_from_exile = false; // re-granted only by a fresh warp-exile (400.7)
            o.play_until_turn = None; // impulse-play window drops on any zone change (400.7)
            o.counter_added_this_turn = false; // counters exist only on the battlefield (110.5d / 400.7)
            o.is_copy = false; // a copy never legitimately changes zones (it ceases to exist, 707.10a)
            o.prepared = false; // a fresh object identity (CR 400.7); the "prepared" status ends on leaving play
            if to == Zone::Battlefield {
                o.controller = to_owner;
                o.summoning_sick = o.chars.is_creature();
                o.timestamp = new_ts.unwrap();
            } else {
                o.controller = o.owner;
                o.summoning_sick = false;
            }
        }
        if to == Zone::Battlefield {
            self.enter_with_loyalty(id); // CR 306.5b, after counters were reset
        }
        if let Some(v) = self.player_mut(to_owner).zone_vec_mut(to) {
            v.push(id);
        }
        // Attachment bookkeeping (CR 400.7 / 701.3): an object leaving the battlefield is no
        // longer attached to anything, and anything attached to *it* becomes unattached. The
        // resulting illegal-attachment cases (aura → graveyard, equipment unattaches) are then
        // handled by the state-based-action pass (CR 704.5m/n/q).
        if from_zone == Zone::Battlefield {
            if let Some(o) = self.objects.get_mut(&id) {
                o.attached_to = None;
            }
            for o in self.objects.values_mut() {
                if o.attached_to == Some(id) {
                    o.attached_to = None;
                }
            }
        }
        // Continuous effects change when a permanent enters or leaves the battlefield.
        if to == Zone::Battlefield || from_zone == Zone::Battlefield {
            self.mark_chars_dirty();
        }
        true
    }

    /// Shuffle a player's library using the replayable RNG (CR 701.24).
    pub fn shuffle_library(&mut self, p: PlayerId) {
        let mut lib = std::mem::take(&mut self.player_mut(p).library);
        self.rng.shuffle(&mut lib);
        self.player_mut(p).library = lib;
    }

    /// Players still in the game (have not lost). In two-player, when this drops to ≤1 the
    /// game is over (CR 104.2a).
    pub fn living_players(&self) -> Vec<PlayerId> {
        self.players
            .iter()
            .filter(|p| !p.has_lost)
            .map(|p| p.id)
            .collect()
    }
}
