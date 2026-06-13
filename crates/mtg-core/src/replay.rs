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

use crate::agent::{CombatView, ObjView, StackObjView};
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
}
