//! The whiteboard: the staged, not-yet-committed batch of atomic mutations the engine intends
//! to apply together (WHITEBOARD_MODEL.md §2.1). Card-agnostic and inspectable. The
//! replacement/prevention pass rewrites these (CR 614/615/616); commit executes the survivors
//! and emits an `Event` per completed action.

use super::target::TokenSpec;
use crate::basics::{CounterKind, DamageKind, Target, Zone, ZonePos};
use crate::ids::{ObjId, PlayerId, StackId};
use serde::{Deserialize, Serialize};

/// A single intended mutation to game state. Atomic and card-agnostic. Grows with the IR
/// vocabulary (WHITEBOARD_MODEL.md §5: "start medium, split when a card forces it").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Destroy {
        obj: ObjId,
        source: Option<ObjId>,
    },
    Sacrifice {
        obj: ObjId,
        by: PlayerId,
    },
    Damage {
        target: Target,
        amount: u32,
        source: ObjId,
        kind: DamageKind,
    },
    Draw {
        player: PlayerId,
        count: u32,
    },
    Mill {
        player: PlayerId,
        count: u32,
    },
    LoseLife {
        player: PlayerId,
        amount: u32,
    },
    GainLife {
        player: PlayerId,
        amount: u32,
    },
    MoveZone {
        obj: ObjId,
        to: Zone,
        pos: ZonePos,
        cause: MoveCause,
    },
    TapUntap {
        obj: ObjId,
        tap: bool,
    },
    AddCounters {
        obj: ObjId,
        kind: CounterKind,
        n: i32,
    },
    CreateToken {
        spec: TokenSpec,
        controller: PlayerId,
    },
    AttachTo {
        attachment: ObjId,
        target: Target,
    },
    Discard {
        player: PlayerId,
        obj: ObjId,
    },
    Exile {
        obj: ObjId,
        source: Option<ObjId>,
    },
    /// Exile a warp-cast permanent at its end step (CR 702.x) and mark it castable from exile on a
    /// later turn — distinct from a plain `Exile` so only warp grants the recast permission.
    WarpExile {
        obj: ObjId,
    },
    /// Grant a continuous effect created by resolution (CR 611) over a fixed set of objects —
    /// "until end of turn" pumps, animations (Earthbend's land→creature), etc. Applied by pushing
    /// a [`crate::chars::ContinuousEffect`] into game state, where the layer system folds it in
    /// alongside printed statics. The affected set is fixed here (resolution already chose it).
    GrantContinuous {
        source: Option<ObjId>,
        controller: PlayerId,
        affected: Vec<ObjId>,
        contributions: Vec<super::ability::StaticContribution>,
        duration: super::condition::Duration,
    },
    /// Arm a delayed triggered ability (CR 603.7): "when [watching] [event], do [actions]". When
    /// the event later occurs the engine puts the delayed ability on the stack carrying `actions`
    /// (concrete, serializable, card-agnostic — no `Effect` tree). One-shot. Earthbend uses this
    /// for "when this dies or is exiled, return it to the battlefield tapped".
    RegisterDelayedTrigger {
        watching: ObjId,
        event: DelayedTriggerEvent,
        controller: PlayerId,
        source: Option<ObjId>,
        actions: Vec<Action>,
    },
}

/// The event a delayed triggered ability (CR 603.7) waits for. A starter vocabulary; grows with
/// the card pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DelayedTriggerEvent {
    /// The watched permanent leaves the battlefield by dying (→ graveyard) or being exiled.
    DiesOrExiled,
    /// The beginning of the next end step after this trigger was armed (CR 513 / warp's "exile this
    /// at the beginning of the next end step"). Fires once, then is consumed.
    AtBeginningOfNextEndStep,
}

/// Why an object is changing zones — distinguishes destruction/sacrifice/bounce/etc. so the
/// right triggers (dies vs. leaves vs. is-put-into-graveyard) and LKI fire (CR 603.6/700.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MoveCause {
    Destroyed,
    Sacrificed,
    StateBasedAction,
    Resolved,
    Countered,
    Returned,
    Discarded,
    Exiled,
    Other,
}

/// Why this whiteboard exists (the "reason" for the staged batch). Drives event tagging.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WbReason {
    /// Resolving the spell/ability with this stack id.
    Resolve(StackId),
    /// Combat damage being dealt (CR 510).
    CombatDamage,
    /// A state-based action batch (CR 704).
    StateBasedActions,
    /// Cleanup-step actions (CR 514).
    Cleanup,
    /// A turn-based action (CR 703), e.g. untap, draw-for-turn.
    TurnBased,
}

/// Resolution-time context an effect carries while it materializes its whiteboard: who's
/// resolving it, the source object, the chosen X, and the chosen targets (CR 608.2). Concrete
/// (no `Effect` tree), so it is snapshot-serializable.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResolutionCtx {
    pub controller: Option<PlayerId>,
    pub source: Option<ObjId>,
    pub x: Option<u32>,
    pub chosen_targets: Vec<Target>,
    /// Indices of the modes chosen for a modal spell/ability (CR 700.2).
    pub chosen_modes: Vec<u32>,
    /// The controller of each chosen target, **snapshotted at resolution start** (parallel to
    /// `chosen_targets`; `None` for non-object targets). Lets `PlayerRef::ControllerOfTarget`
    /// resolve to a target's controller even after that object left play during this resolution
    /// (e.g. Erode's "Destroy target … its controller may search"). Empty if not captured.
    pub target_controllers: Vec<Option<PlayerId>>,
}

/// A staged batch of actions the engine intends to apply together (the "nap"): materialize →
/// rewrite pass → commit (WHITEBOARD_MODEL.md §2.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Whiteboard {
    pub reason: WbReason,
    /// Ordered; the rewrite pass may erase / replace / insert entries (CR 614.5/616).
    pub actions: Vec<Action>,
    pub ctx: ResolutionCtx,
}

impl Whiteboard {
    pub fn new(reason: WbReason, ctx: ResolutionCtx) -> Self {
        Whiteboard {
            reason,
            actions: Vec::new(),
            ctx,
        }
    }

    pub fn push(&mut self, action: Action) {
        self.actions.push(action);
    }
}
