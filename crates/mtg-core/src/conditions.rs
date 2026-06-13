//! Evaluating an `effects::condition::Condition` against current game state (CR 603.4 intervening
//! "if", activation `Restriction::OnlyIf`, etc.). Pure read of [`GameState`]; never mutates.
//!
//! A condition is evaluated relative to a *source controller* (the "you" in "if you control …").
//! `ValueExpr`s inside conditions are read against base characteristics (no resolution context),
//! which covers the fixed/count/sum cases conditions actually use.

use crate::effects::condition::Condition;
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::ids::PlayerId;
use crate::state::{Characteristics, GameState};

/// Whether `cond` holds right now, with "you"/`PlayerRef::Controller` = `source_controller`.
pub(crate) fn holds(state: &GameState, cond: &Condition, source_controller: PlayerId) -> bool {
    match cond {
        Condition::Always => true,
        Condition::Not(c) => !holds(state, c, source_controller),
        Condition::All(cs) => cs.iter().all(|c| holds(state, c, source_controller)),
        Condition::AnyOf(cs) => cs.iter().any(|c| holds(state, c, source_controller)),
        Condition::YourTurn => state.active_player == source_controller,
        Condition::CountAtLeast { zone, filter, controller, n } => {
            let want = controller.map(|r| resolve_player(state, r, source_controller));
            let count = state
                .objects
                .values()
                .filter(|o| o.zone == *zone)
                .filter(|o| want.is_none_or(|p| o.controller == p))
                .filter(|o| filter_matches(&o.chars, filter))
                .count() as i64;
            count >= eval_value(state, n, source_controller)
        }
        Condition::LifeAtLeast { who, n } => {
            let p = resolve_player(state, *who, source_controller);
            life(state, p) >= eval_value(state, n, source_controller)
        }
        Condition::LifeAtMost { who, n } => {
            let p = resolve_player(state, *who, source_controller);
            life(state, p) <= eval_value(state, n, source_controller)
        }
        Condition::ValueAtLeast(a, b) => {
            eval_value(state, a, source_controller) >= eval_value(state, b, source_controller)
        }
    }
}

fn life(state: &GameState, p: PlayerId) -> i64 {
    state.players.get(p.0 as usize).map(|pl| pl.life as i64).unwrap_or(0)
}

fn resolve_player(state: &GameState, r: PlayerRef, source_controller: PlayerId) -> PlayerId {
    match r {
        PlayerRef::Opponent | PlayerRef::EachOpponent => state
            .players
            .iter()
            .map(|p| p.id)
            .find(|&q| q != source_controller)
            .unwrap_or(source_controller),
        _ => source_controller, // Controller / Owner / others
    }
}

/// Minimal `ValueExpr` eval for conditions (no resolution context): the fixed/count/sum subset
/// conditions use. Other variants read as 0.
fn eval_value(state: &GameState, v: &ValueExpr, source_controller: PlayerId) -> i64 {
    match v {
        ValueExpr::Fixed(n) => *n,
        ValueExpr::Sum(a, b) => {
            eval_value(state, a, source_controller) + eval_value(state, b, source_controller)
        }
        ValueExpr::Count { zone, filter, controller } => {
            let want = controller.map(|r| resolve_player(state, r, source_controller));
            state
                .objects
                .values()
                .filter(|o| o.zone == *zone)
                .filter(|o| want.is_none_or(|p| o.controller == p))
                .filter(|o| filter_matches(&o.chars, filter))
                .count() as i64
        }
        _ => 0,
    }
}

/// Evaluate a `CardFilter` against an object's BASE characteristics — the subset conditions use
/// (`ControlledBy` is handled by the surrounding controller restriction).
fn filter_matches(chars: &Characteristics, filter: &CardFilter) -> bool {
    match filter {
        CardFilter::Any | CardFilter::ControlledBy(_) => true,
        CardFilter::HasCardType(t) => chars.card_types.contains(t),
        CardFilter::Supertype(s) => chars.supertypes.contains(s),
        CardFilter::HasSubtype(s) => chars.subtypes.contains(s),
        CardFilter::HasColor(c) => chars.colors.contains(c),
        CardFilter::Colorless => chars.colors.is_empty(),
        CardFilter::All(fs) => fs.iter().all(|f| filter_matches(chars, f)),
        CardFilter::AnyOf(fs) => fs.iter().any(|f| filter_matches(chars, f)),
        CardFilter::Not(f) => !filter_matches(chars, f),
        _ => false,
    }
}
