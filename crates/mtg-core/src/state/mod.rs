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

use crate::basics::{CardType, Color, CounterBag, ManaCost, ManaPool, Phase, Status, Zone};
use crate::cards::{CardDb, CardDef};
use crate::combat::CombatState;
use crate::ids::{ObjId, PlayerId};
use crate::rng::Rng;
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
    pub subtypes: Vec<String>,
    pub supertypes: Vec<String>,
    pub colors: Vec<Color>,
    pub mana_cost: Option<ManaCost>,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
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
    /// Mana value (CR 202.3): generic + the sum of all colored pips.
    pub fn mana_value(&self) -> u32 {
        match &self.mana_cost {
            Some(c) => c.generic + c.colored.values().copied().sum::<u32>(),
            None => 0,
        }
    }

    /// A basic land card (no abilities yet; mana abilities arrive in milestone 3).
    pub fn basic_land(name: &str) -> Self {
        Characteristics {
            name: name.to_string(),
            card_types: vec![CardType::Land],
            supertypes: vec!["Basic".to_string()],
            subtypes: vec![name.to_string()],
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
    /// Summoning sickness (CR 302.6): can't attack / use `{T}` until controlled since the
    /// start of its controller's most recent turn (unless it has haste).
    pub summoning_sick: bool,
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
    pub mana_pool: ManaPool,
    pub counters: CounterBag,
    /// Lands played this turn (CR 116.2a / 505.6b: one per turn by default).
    pub lands_played_this_turn: u32,
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
            mana_pool: ManaPool::default(),
            counters: CounterBag::default(),
            lands_played_this_turn: 0,
            hand_size_limit: DEFAULT_HAND_SIZE,
            has_lost: false,
            drew_from_empty: false,
        }
    }

    /// The owned `ObjId` vector for a per-player zone (everything except `Stack`/`Command`).
    fn zone_vec_mut(&mut self, zone: Zone) -> Option<&mut Vec<ObjId>> {
        match zone {
            Zone::Library => Some(&mut self.library),
            Zone::Hand => Some(&mut self.hand),
            Zone::Battlefield => Some(&mut self.battlefield),
            Zone::Graveyard => Some(&mut self.graveyard),
            Zone::Exile => Some(&mut self.exile),
            Zone::Stack | Zone::Command => None,
        }
    }
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
    /// Combat state during a combat phase (CR 506–511); `None` outside combat.
    pub combat: Option<CombatState>,
    pub game_over: bool,
    pub winner: Option<PlayerId>,
    pub rng: Rng,
    /// The card definitions (abilities = Effect IR) for cards in this game. Card *data*, not
    /// snapshot state: shared via `Arc` (clone is O(1)) and **not serialized** (a snapshot
    /// re-attaches the db on load). Looked up by object `grp_id`.
    #[serde(skip)]
    pub card_db: Arc<CardDb>,
    next_obj: u64,
    next_stack: u64,
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
            combat: None,
            game_over: false,
            winner: None,
            rng: Rng::new(seed),
            card_db: Arc::new(CardDb::default()),
            next_obj: 1,
            next_stack: 1,
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

    /// Create an object owned by `owner` and place it (appended) into one of that player's
    /// zones. Returns its id. Used to build decks.
    pub fn add_card(&mut self, owner: PlayerId, chars: Characteristics, zone: Zone) -> ObjId {
        let id = self.mint_obj();
        let obj = Object {
            id,
            owner,
            controller: owner,
            zone,
            chars,
            status: Status::default(),
            counters: CounterBag::default(),
            damage_marked: 0,
            summoning_sick: false,
        };
        self.objects.insert(id, obj);
        if let Some(v) = self.player_mut(owner).zone_vec_mut(zone) {
            v.push(id);
        }
        id
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
    pub(crate) fn move_object(&mut self, id: ObjId, to: Zone, to_owner: PlayerId) -> bool {
        let (from_zone, from_owner) = match self.objects.get(&id) {
            Some(o) => (o.zone, o.owner),
            None => return false,
        };
        // Remove from the source zone vector.
        if let Some(v) = self.player_mut(from_owner).zone_vec_mut(from_zone) {
            if let Some(pos) = v.iter().position(|&x| x == id) {
                v.remove(pos);
            }
        }
        // Update the object, then append to the destination zone vector.
        if let Some(o) = self.objects.get_mut(&id) {
            o.zone = to;
            // A permanent enters untapped/unflipped/face-up/phased-in (CR 110.5b); status,
            // counters and marked damage exist only on the battlefield (CR 110.5d), so reset
            // them on every zone change either way.
            o.status = Status::default();
            o.counters = CounterBag::default();
            o.damage_marked = 0;
            if to == Zone::Battlefield {
                o.controller = to_owner;
                o.summoning_sick = o.chars.is_creature();
            } else {
                o.controller = o.owner;
                o.summoning_sick = false;
            }
        }
        if let Some(v) = self.player_mut(to_owner).zone_vec_mut(to) {
            v.push(id);
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
