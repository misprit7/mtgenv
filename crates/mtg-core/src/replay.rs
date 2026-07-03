//! Replay contract — the shared serde types for game replays + omniscient spectating
//! (`docs/plans/REPLAY_PLAN.md`). The engine is deterministic and `GameState` is `Clone`, so a
//! replay is a recorded **stream of omniscient snapshots** ([`GodView`]): the viewer is a dumb
//! frame-player with no engine dependency, and scrubbing is trivial.
//!
//! This module owns the contract everyone speaks: `mtg-core` produces it (via
//! [`crate::state::view::god_view`] + [`crate::priority::Engine::record_replay`]), webui serves +
//! renders it, gym writes training replays. All zones reuse [`ObjView`]/[`CharacteristicsView`]
//! so the web board code renders a `GodView` with the same machinery as a `PlayerView`.

use serde::{Deserialize, Serialize};

use crate::agent::{CharacteristicsView, CombatView, ObjView, StackObjView};
use crate::basics::{Status, Zone};
use crate::ids::ObjId;
use crate::basics::{CounterBag, ManaPool, Phase};
use crate::ids::PlayerId;
use crate::priority::Outcome;

/// An omniscient, no-hidden-information view of the whole game (CR-agnostic projection). Built by
/// [`crate::state::view::god_view`] for spectators and replays: **every zone of every player is
/// fully visible**, including each library *in order* (top-first). The battlefield is a flat list
/// across all controllers (like [`crate::agent::PlayerView::battlefield`]); the masked, per-player
/// zones (hand/library/graveyard/exile) live under [`GodPlayerView`]. Every object is the
/// `ObjView::Visible` variant — nothing is a `Hidden` stub — so spectators see all face-up.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GodView {
    pub turn: u32,
    pub active_player: PlayerId,
    pub phase: Phase,
    pub priority_player: Option<PlayerId>,
    /// Every seat, with all of its zones fully visible.
    pub players: Vec<GodPlayerView>,
    /// All permanents on the battlefield, face-up (flat; each `ObjView` carries its controller).
    pub battlefield: Vec<ObjView>,
    pub stack: Vec<StackObjView>,
    pub combat: Option<CombatView>,
}

/// One seat's fully-unmasked zones (omniscient). `library` is **ordered, top of library first**
/// (the hidden information a [`crate::agent::PlayerView`] collapses to a count).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GodPlayerView {
    pub player: PlayerId,
    pub life: i32,
    pub poison: u32,
    pub mana_pool: ManaPool,
    pub counters: CounterBag,
    pub hand: Vec<ObjView>,
    /// The library in order — index 0 is the top of the library.
    pub library: Vec<ObjView>,
    pub graveyard: Vec<ObjView>,
    pub exile: Vec<ObjView>,
}

/// A recorded game as a stream of omniscient snapshots — the shared replay contract (engine owns
/// it; webui + gym consume). Snapshots, not seed+decisions, so the viewer needs no engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replay {
    pub meta: ReplayMeta,
    pub frames: Vec<ReplayFrame>,
}

/// One recorded step: the omniscient board after something happened, plus a human label for it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayFrame {
    pub state: GodView,
    /// What just happened, e.g. `"P0 casts Lightning Bolt"`, `"Turn 3 — P0 upkeep"`.
    pub label: String,
}

/// Replay metadata. The engine fills `players` (seats) + `result`; everything else is
/// **caller-stamped from outside** (no clock in the core): `source`, `created_at`, and the player
/// names/decks. webui/gym set those when they persist the replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayMeta {
    pub players: Vec<ReplayPlayer>,
    /// `None` until the game finishes; then the engine's [`Outcome`] (winner / turns / reason).
    pub result: Option<Outcome>,
    pub source: ReplaySource,
    /// Unix epoch milliseconds, stamped by the caller (`0` = unset; the core never reads a clock).
    pub created_at: i64,
}

/// Per-seat replay metadata (caller fills `name`/`deck`; the engine seeds `seat`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayPlayer {
    pub seat: PlayerId,
    pub name: String,
    pub deck: String,
}

/// Where a replay came from. Serializes externally-tagged: `"Human"` or
/// `{"AiTraining": {"step": 1200}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplaySource {
    /// A human (or mixed human/AI) game played in the lobby.
    Human,
    /// A self-play game sampled during training at the given update/step.
    AiTraining { step: u64 },
}

impl ReplayMeta {
    /// A bare metadata stub for `n` seats: seats `0..n` with empty names/decks, no result,
    /// `source` as given, `created_at = 0`. The caller overwrites the fields it knows.
    pub fn new(n: usize, source: ReplaySource) -> Self {
        ReplayMeta {
            players: (0..n)
                .map(|i| ReplayPlayer {
                    seat: PlayerId(i as u32),
                    name: String::new(),
                    deck: String::new(),
                })
                .collect(),
            result: None,
            source,
            created_at: 0,
        }
    }
}

// ── Compact (delta) replay wire format — v2 ─────────────────────────────────────────────────
//
// A full [`GodView`] per frame is dominated by each seat's **ordered library** (~40 objects) plus
// the battlefield — zones that change rarely, whose objects carry a big [`CharacteristicsView`]
// (oracle text) that never changes — so raw replays run 15–73 MB (~60 KB/frame). Two wins compose:
//  1. **Zone delta:** keep the small scalars (turn/phase/priority/life/mana/counters/stack/combat)
//     in full each frame but **omit any large Vec zone** (battlefield + each seat's hand/library/
//     graveyard/exile) byte-identical to the previous frame. Frame 0 is a full keyframe.
//  2. **Characteristics dedup:** intern each unique `CharacteristicsView` once in
//     [`CompactReplay::chars_dict`]; the per-frame objects ([`CompactObj`]) reference it by index
//     instead of inlining the oracle text every time.
// Measured together on a real 348-frame game: **23.3 MB → 0.5 MB (~46×)**. Reconstruction carries
// each omitted zone forward and re-inlines characteristics from the dictionary.
//
// Consumers serialize [`Replay::to_compact`] for the slim on-disk/wire form and read either format
// via [`AnyReplay`] (v2 compact OR the pre-v2 full-frame files) — so **old replays still load**.
// The in-memory [`Replay`]/[`ReplayFrame`]/[`GodView`] types are unchanged, so every consumer that
// walks full frames (the frame player, spectator streaming) is untouched.

/// Format version for [`CompactReplay`]; its presence in the JSON distinguishes compact from the
/// pre-v2 full-frame files. Bump when the compact schema changes.
pub const COMPACT_REPLAY_VERSION: u32 = 2;

/// The compact, delta-encoded wire form of a [`Replay`] (see the module notes). Build with
/// [`Replay::to_compact`]; reconstruct the full replay with [`CompactReplay::into_replay`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactReplay {
    /// Always [`COMPACT_REPLAY_VERSION`]. A required field so a legacy (versionless) file can't be
    /// mistaken for compact by [`AnyReplay`].
    pub version: u32,
    pub meta: ReplayMeta,
    /// Interned object characteristics (name/types/oracle-text/P-T/…). An object's `CharacteristicsView`
    /// is large (oracle text) and repeats across every frame it appears in; the compact objects
    /// reference this dictionary by index instead of inlining it, which is the bulk of the size win.
    #[serde(rename = "chars")]
    pub chars_dict: Vec<CharacteristicsView>,
    pub frames: Vec<CompactFrame>,
}

/// One delta frame: full scalars + stack/combat, with the large Vec zones present only when they
/// changed from the previous frame (absent ⇒ unchanged; carried forward on reconstruction).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactFrame {
    pub label: String,
    pub turn: u32,
    pub active_player: PlayerId,
    pub phase: Phase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority_player: Option<PlayerId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<StackObjView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat: Option<CombatView>,
    pub players: Vec<CompactPlayer>,
    /// `None` ⇒ the battlefield is unchanged from the previous frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battlefield: Option<Vec<CompactObj>>,
}

/// One seat in a [`CompactFrame`]: scalars in full, the four hidden zones delta-encoded
/// (`None` ⇒ unchanged from the previous frame).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactPlayer {
    pub player: PlayerId,
    pub life: i32,
    pub poison: u32,
    pub mana_pool: ManaPool,
    pub counters: CounterBag,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand: Option<Vec<CompactObj>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<Vec<CompactObj>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graveyard: Option<Vec<CompactObj>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exile: Option<Vec<CompactObj>>,
}

/// A lean [`ObjView`] for the compact format: identical to `ObjView` except the big
/// [`CharacteristicsView`] is replaced by an index (`c`) into [`CompactReplay::chars_dict`], and
/// the usually-empty fields are omitted. Short field names because these objects are the most
/// repeated thing in a replay. Purely an on-disk shape — reconstructed to a full `ObjView`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompactObj {
    #[serde(rename = "v")]
    Visible {
        #[serde(rename = "i")]
        id: ObjId,
        /// Index into [`CompactReplay::chars_dict`].
        #[serde(rename = "c")]
        chars: u32,
        #[serde(rename = "k")]
        controller: PlayerId,
        #[serde(rename = "o")]
        owner: PlayerId,
        #[serde(rename = "z")]
        zone: Zone,
        #[serde(rename = "s")]
        status: Status,
        #[serde(rename = "n", default, skip_serializing_if = "counters_empty")]
        counters: CounterBag,
        #[serde(rename = "d", default, skip_serializing_if = "is_zero_u32")]
        damage_marked: u32,
        #[serde(rename = "a", default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<ObjId>,
        #[serde(rename = "m", default, skip_serializing_if = "is_false")]
        summoning_sick: bool,
    },
    #[serde(rename = "h")]
    Hidden {
        #[serde(rename = "i")]
        id: ObjId,
        #[serde(rename = "z")]
        zone: Zone,
        #[serde(rename = "k")]
        controller: PlayerId,
    },
}

fn counters_empty(c: &CounterBag) -> bool {
    c.counts.is_empty()
}
fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}
fn is_false(b: &bool) -> bool {
    !*b
}

/// Interns [`CharacteristicsView`] values into a de-duplicated dictionary (bucketed by the cheap
/// `(grp_id, power, toughness)` key, then exact-matched — the full content is what varies, e.g.
/// granted keywords). Returns the stable index used by [`CompactObj`].
#[derive(Default)]
struct CharsInterner {
    dict: Vec<CharacteristicsView>,
    buckets: std::collections::HashMap<(u32, Option<i32>, Option<i32>), Vec<u32>>,
}
impl CharsInterner {
    fn intern(&mut self, c: &CharacteristicsView) -> u32 {
        let key = (c.grp_id, c.power, c.toughness);
        if let Some(bucket) = self.buckets.get(&key) {
            for &i in bucket {
                if &self.dict[i as usize] == c {
                    return i;
                }
            }
        }
        let i = self.dict.len() as u32;
        self.dict.push(c.clone());
        self.buckets.entry(key).or_default().push(i);
        i
    }
}

/// Lower a full [`ObjView`] to a [`CompactObj`], interning its characteristics.
fn compact_obj(interner: &mut CharsInterner, o: &ObjView) -> CompactObj {
    match o {
        ObjView::Visible {
            id, chars, controller, owner, zone, status, counters, damage_marked, attachments, summoning_sick,
        } => CompactObj::Visible {
            id: *id,
            chars: interner.intern(chars),
            controller: *controller,
            owner: *owner,
            zone: *zone,
            status: *status,
            counters: counters.clone(),
            damage_marked: *damage_marked,
            attachments: attachments.clone(),
            summoning_sick: *summoning_sick,
        },
        ObjView::Hidden { id, zone, controller } => {
            CompactObj::Hidden { id: *id, zone: *zone, controller: *controller }
        }
    }
}

/// Reconstruct a full [`ObjView`] from a [`CompactObj`] using the chars dictionary.
fn full_obj(dict: &[CharacteristicsView], o: &CompactObj) -> ObjView {
    match o {
        CompactObj::Visible {
            id, chars, controller, owner, zone, status, counters, damage_marked, attachments, summoning_sick,
        } => ObjView::Visible {
            id: *id,
            chars: dict[*chars as usize].clone(),
            controller: *controller,
            owner: *owner,
            zone: *zone,
            status: *status,
            counters: counters.clone(),
            damage_marked: *damage_marked,
            attachments: attachments.clone(),
            summoning_sick: *summoning_sick,
        },
        CompactObj::Hidden { id, zone, controller } => {
            ObjView::Hidden { id: *id, zone: *zone, controller: *controller }
        }
    }
}

/// `Some(lean objects)` if `cur` differs from `prev` (or there is no previous frame), `None` if
/// identical (so the encoder omits an unchanged zone and reconstruction carries it forward). An
/// empty zone (`[]`) is distinct from `None`/absent.
fn delta_zone(
    interner: &mut CharsInterner,
    prev: Option<&Vec<ObjView>>,
    cur: &[ObjView],
) -> Option<Vec<CompactObj>> {
    match prev {
        Some(p) if p.as_slice() == cur => None,
        _ => Some(cur.iter().map(|o| compact_obj(interner, o)).collect()),
    }
}

impl Replay {
    /// Delta-encode into the compact wire form (module notes) — the slim thing to serialize to
    /// disk / send over the wire (100×+ smaller raw). Reconstruct with [`CompactReplay::into_replay`].
    pub fn to_compact(&self) -> CompactReplay {
        let mut interner = CharsInterner::default();
        let mut frames = Vec::with_capacity(self.frames.len());
        let mut prev: Option<&GodView> = None;
        for f in &self.frames {
            let g = &f.state;
            let players = g
                .players
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let pp = prev.and_then(|pg| pg.players.get(i));
                    CompactPlayer {
                        player: p.player,
                        life: p.life,
                        poison: p.poison,
                        mana_pool: p.mana_pool.clone(),
                        counters: p.counters.clone(),
                        hand: delta_zone(&mut interner, pp.map(|x| &x.hand), &p.hand),
                        library: delta_zone(&mut interner, pp.map(|x| &x.library), &p.library),
                        graveyard: delta_zone(&mut interner, pp.map(|x| &x.graveyard), &p.graveyard),
                        exile: delta_zone(&mut interner, pp.map(|x| &x.exile), &p.exile),
                    }
                })
                .collect();
            let battlefield = delta_zone(&mut interner, prev.map(|pg| &pg.battlefield), &g.battlefield);
            frames.push(CompactFrame {
                label: f.label.clone(),
                turn: g.turn,
                active_player: g.active_player,
                phase: g.phase,
                priority_player: g.priority_player,
                stack: g.stack.clone(),
                combat: g.combat.clone(),
                players,
                battlefield,
            });
            prev = Some(g);
        }
        CompactReplay {
            version: COMPACT_REPLAY_VERSION,
            meta: self.meta.clone(),
            chars_dict: interner.dict,
            frames,
        }
    }
}

impl CompactReplay {
    /// Reconstruct the full-frame [`Replay`] (module notes), carrying each omitted (unchanged) zone
    /// forward from the previous frame. Inverse of [`Replay::to_compact`].
    pub fn into_replay(self) -> Replay {
        let dict = &self.chars_dict;
        // Resolve a delta zone: `Some(lean)` ⇒ rebuild full objects via the dict; `None` ⇒ carry the
        // previously-reconstructed (already-full) zone forward.
        let resolve = |delta: Option<Vec<CompactObj>>, carried: Option<&Vec<ObjView>>| -> Vec<ObjView> {
            match delta {
                Some(lean) => lean.iter().map(|o| full_obj(dict, o)).collect(),
                None => carried.cloned().unwrap_or_default(),
            }
        };
        let mut frames = Vec::with_capacity(self.frames.len());
        let mut prev: Option<GodView> = None;
        for cf in self.frames {
            let players = cf
                .players
                .into_iter()
                .enumerate()
                .map(|(i, cp)| {
                    let pp = prev.as_ref().and_then(|pg| pg.players.get(i));
                    GodPlayerView {
                        player: cp.player,
                        life: cp.life,
                        poison: cp.poison,
                        mana_pool: cp.mana_pool,
                        counters: cp.counters,
                        hand: resolve(cp.hand, pp.map(|x| &x.hand)),
                        library: resolve(cp.library, pp.map(|x| &x.library)),
                        graveyard: resolve(cp.graveyard, pp.map(|x| &x.graveyard)),
                        exile: resolve(cp.exile, pp.map(|x| &x.exile)),
                    }
                })
                .collect();
            let battlefield = resolve(cf.battlefield, prev.as_ref().map(|pg| &pg.battlefield));
            let g = GodView {
                turn: cf.turn,
                active_player: cf.active_player,
                phase: cf.phase,
                priority_player: cf.priority_player,
                players,
                battlefield,
                stack: cf.stack,
                combat: cf.combat,
            };
            frames.push(ReplayFrame { state: g.clone(), label: cf.label });
            prev = Some(g);
        }
        Replay { meta: self.meta, frames }
    }
}

/// Reads EITHER a v2 [`CompactReplay`] or a pre-v2 full-frame [`Replay`] (old saved files), so old
/// replays keep loading. Deserialize this from disk/wire, then [`AnyReplay::into_replay`].
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AnyReplay {
    /// v2 compact (discriminated by its required `version` field).
    Compact(CompactReplay),
    /// Pre-v2 full-frame replay (no `version`; each frame carries a full `state`).
    Legacy(Replay),
}

impl AnyReplay {
    /// The reconstructed full-frame replay, whichever format it was stored in.
    pub fn into_replay(self) -> Replay {
        match self {
            AnyReplay::Compact(c) => c.into_replay(),
            AnyReplay::Legacy(r) => r,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_source_wire_shape_is_locked() {
        // webui + gym depend on this externally-tagged JSON shape.
        assert_eq!(serde_json::to_string(&ReplaySource::Human).unwrap(), "\"Human\"");
        assert_eq!(
            serde_json::to_string(&ReplaySource::AiTraining { step: 1200 }).unwrap(),
            r#"{"AiTraining":{"step":1200}}"#
        );
    }

    #[test]
    fn replay_meta_stub_is_caller_fillable() {
        let m = ReplayMeta::new(2, ReplaySource::Human);
        assert_eq!(m.players.len(), 2);
        assert_eq!(m.players[1].seat, PlayerId(1));
        assert!(m.players[0].name.is_empty() && m.players[0].deck.is_empty());
        assert!(m.result.is_none(), "no result until finished");
        assert_eq!(m.created_at, 0, "clock stamped from outside");
    }

    #[test]
    fn replay_with_counters_round_trips_through_json() {
        // Regression for the Selesnya-replay crash: a `CounterBag` is a `BTreeMap` keyed by
        // `CounterKind`, and `serde_json` panics ("key must be a string") on the non-string keys the
        // derived enum repr produces for `Keyword`/`Named` counters (e.g. a quest counter). A
        // permanent carrying counters appears in every replay frame's `GodView.battlefield`, so this
        // broke webui lobby replays AND gym training-replay export. The fix keys the map by each
        // `CounterKind`'s canonical `Display` string.
        use crate::basics::{CardType, CounterKind, Zone};
        use crate::state::view::god_view;
        use crate::state::{Characteristics, GameState};

        let mut state = GameState::new(2, 1);
        let bear = state.add_card(
            PlayerId(0),
            Characteristics {
                name: "Counter Bear".into(),
                card_types: vec![CardType::Creature],
                power: Some(2),
                toughness: Some(2),
                ..Default::default()
            },
            Zone::Battlefield,
        );
        // A built-in (unit-variant) counter AND a data-carrying `Named` counter — the latter is the
        // one `serde_json` rejected as a non-string map key before the string-keyed adapter.
        {
            let o = state.objects.get_mut(&bear).unwrap();
            o.counters.counts.insert(CounterKind::PlusOnePlusOne, 3);
            o.counters.counts.insert(CounterKind::Named("quest".into()), 1);
        }

        let replay = Replay {
            meta: ReplayMeta::new(2, ReplaySource::Human),
            frames: vec![ReplayFrame {
                state: god_view(&state),
                label: "P0 has a counter-bearing bear".into(),
            }],
        };

        // Before the fix this PANICKED; now it is clean JSON that round-trips.
        let json = serde_json::to_string(&replay).expect("a replay with counters must serialize");
        let back: Replay = serde_json::from_str(&json).expect("and deserialize");

        let counters = match &back.frames[0].state.battlefield[0] {
            ObjView::Visible { counters, .. } => counters,
            other => panic!("expected a Visible permanent, got {other:?}"),
        };
        // The wire format keys the counter map by each kind's canonical string (a valid JSON object):
        // built-ins keep their variant-name key (unchanged from the old repr; the web client's
        // CTR_LABEL still maps it), and the `Named` quest counter is the bare name.
        expect_test::expect![[r#"{"counts":{"PlusOnePlusOne":3,"quest":1}}"#]]
            .assert_eq(&serde_json::to_string(counters).unwrap());
        // And the typed values survive the round-trip unchanged.
        assert_eq!(counters.get(&CounterKind::PlusOnePlusOne), 3);
        assert_eq!(counters.get(&CounterKind::Named("quest".into())), 1);
    }

    // ── Compact (delta) format ──────────────────────────────────────────────────────────────

    /// A chunky `ObjView` (with oracle text, like a real card) so the size test is meaningful.
    fn ov(id: u64, zone: Zone) -> ObjView {
        ObjView::Visible {
            id: ObjId(id),
            chars: CharacteristicsView {
                name: format!("Card {id}"),
                grp_id: id as u32,
                rules_text: "whenever this creature attacks, do a thing. ".repeat(3),
                ..Default::default()
            },
            controller: PlayerId(0),
            owner: PlayerId(0),
            zone,
            status: Status::default(),
            counters: CounterBag::default(),
            damage_marked: 0,
            attachments: Vec::new(),
            summoning_sick: false,
        }
    }

    fn seat(s: u32, life: i32, library: Vec<ObjView>, hand: Vec<ObjView>) -> GodPlayerView {
        GodPlayerView {
            player: PlayerId(s),
            life,
            poison: 0,
            mana_pool: ManaPool::default(),
            counters: CounterBag::default(),
            hand,
            library,
            graveyard: Vec::new(),
            exile: Vec::new(),
        }
    }

    /// `to_compact` → `into_replay` reconstructs the replay exactly (lossless), and the compact
    /// form is far smaller because unchanged libraries/battlefield aren't re-stored per frame.
    #[test]
    fn compact_delta_round_trips_and_shrinks() {
        // Constant 40-card libraries — the per-frame redundancy the delta kills.
        let p0_lib: Vec<ObjView> = (0..40).map(|i| ov(i, Zone::Library)).collect();
        let p1_lib: Vec<ObjView> = (100..140).map(|i| ov(i, Zone::Library)).collect();
        let land = ov(500, Zone::Battlefield);

        let gv = |turn, phase, life0, bf: Vec<ObjView>, p0lib: Vec<ObjView>, p0hand: Vec<ObjView>| GodView {
            turn,
            active_player: PlayerId(0),
            phase,
            priority_player: Some(PlayerId(0)),
            players: vec![seat(0, life0, p0lib, p0hand), seat(1, 20, p1_lib.clone(), Vec::new())],
            battlefield: bf,
            stack: Vec::new(),
            combat: None,
        };

        let mut frames = vec![ReplayFrame {
            state: gv(1, Phase::PrecombatMain, 20, Vec::new(), p0_lib.clone(), Vec::new()),
            label: "turn 1 main".into(),
        }];
        // 16 "nothing big changed" frames (only life ticks) — the common case in a real game.
        for i in 0..16 {
            frames.push(ReplayFrame {
                state: gv(1, Phase::DeclareAttackers, 20 - i, Vec::new(), p0_lib.clone(), Vec::new()),
                label: format!("chip {i}"),
            });
        }
        // A land drop: battlefield changes, libraries don't.
        frames.push(ReplayFrame {
            state: gv(1, Phase::PostcombatMain, 4, vec![land.clone()], p0_lib.clone(), Vec::new()),
            label: "played a land".into(),
        });
        // A draw: P0's library + hand change, battlefield + P1 don't.
        frames.push(ReplayFrame {
            state: gv(2, Phase::PrecombatMain, 4, vec![land.clone()], p0_lib[1..].to_vec(), vec![ov(0, Zone::Hand)]),
            label: "turn 2 draw".into(),
        });
        let replay = Replay { meta: ReplayMeta::new(2, ReplaySource::Human), frames };

        // Lossless: compact → reconstruct equals the original (compared via the full-frame JSON).
        let full_json = serde_json::to_string(&replay).unwrap();
        let compact = replay.to_compact();
        let reconstructed = compact.clone().into_replay();
        assert_eq!(
            full_json,
            serde_json::to_string(&reconstructed).unwrap(),
            "compact→reconstruct must be lossless"
        );

        // Big shrink: unchanged libraries are stored ~twice (keyframe + the draw), not 19×.
        let compact_json = serde_json::to_string(&compact).unwrap();
        assert!(
            compact_json.len() * 5 < full_json.len(),
            "compact ({}) should be far smaller than full ({})",
            compact_json.len(),
            full_json.len()
        );

        // The delta actually omitted unchanged zones: a life-only-change frame carries no zone Vecs.
        let mid = &compact.frames[5];
        assert!(mid.battlefield.is_none());
        assert!(mid.players.iter().all(|p| p.library.is_none() && p.hand.is_none()));
        // The draw frame re-sends P0's library + hand, but not the battlefield or P1's library.
        let draw = compact.frames.last().unwrap();
        assert!(draw.players[0].library.is_some() && draw.players[0].hand.is_some());
        assert!(draw.battlefield.is_none() && draw.players[1].library.is_none());
    }

    /// `AnyReplay` reads BOTH the new v2 compact JSON and the pre-v2 full-frame JSON to the same
    /// replay — so old saved files keep loading (versioned format).
    #[test]
    fn any_replay_reads_v2_compact_and_legacy_full_frames() {
        let replay = Replay {
            meta: ReplayMeta::new(2, ReplaySource::Human),
            frames: vec![
                ReplayFrame {
                    state: GodView {
                        turn: 1,
                        active_player: PlayerId(0),
                        phase: Phase::PrecombatMain,
                        priority_player: None,
                        players: vec![seat(0, 20, vec![ov(1, Zone::Library)], Vec::new()), seat(1, 20, Vec::new(), Vec::new())],
                        battlefield: Vec::new(),
                        stack: Vec::new(),
                        combat: None,
                    },
                    label: "start".into(),
                },
                ReplayFrame {
                    state: GodView {
                        turn: 1,
                        active_player: PlayerId(0),
                        phase: Phase::DeclareAttackers,
                        priority_player: Some(PlayerId(1)),
                        players: vec![seat(0, 18, vec![ov(1, Zone::Library)], Vec::new()), seat(1, 20, Vec::new(), Vec::new())],
                        battlefield: Vec::new(),
                        stack: Vec::new(),
                        combat: None,
                    },
                    label: "combat".into(),
                },
            ],
        };
        let expected = serde_json::to_string(&replay).unwrap();

        // v2 compact JSON round-trips through AnyReplay.
        let compact_json = serde_json::to_string(&replay.to_compact()).unwrap();
        let via_compact: AnyReplay = serde_json::from_str(&compact_json).unwrap();
        assert!(matches!(via_compact, AnyReplay::Compact(_)), "v2 JSON parses as the compact variant");
        assert_eq!(expected, serde_json::to_string(&via_compact.into_replay()).unwrap());

        // Legacy (pre-v2) full-frame JSON — the exact shape older files on disk have — still loads.
        let legacy_json = serde_json::to_string(&replay).unwrap(); // Replay's derived serde = full frames
        let via_legacy: AnyReplay = serde_json::from_str(&legacy_json).unwrap();
        assert!(matches!(via_legacy, AnyReplay::Legacy(_)), "versionless JSON parses as legacy");
        assert_eq!(expected, serde_json::to_string(&via_legacy.into_replay()).unwrap());
    }
}
