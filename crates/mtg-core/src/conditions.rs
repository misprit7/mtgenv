//! Evaluating an `effects::condition::Condition` against current game state (CR 603.4 intervening
//! "if", activation `Restriction::OnlyIf`, etc.). Pure read of [`GameState`]; never mutates.
//!
//! A condition is evaluated relative to a *source controller* (the "you" in "if you control …").
//! `ValueExpr`s inside conditions are read against base characteristics (no resolution context),
//! which covers the fixed/count/sum cases conditions actually use.

use std::collections::BTreeSet;

use crate::basics::{CardType, Zone};
use crate::effects::condition::Condition;
use crate::effects::target::CardFilter;
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::ids::{ObjId, PlayerId};
use crate::state::{Characteristics, GameState};

/// Whether `cond` holds right now, with "you"/`PlayerRef::Controller` = `source_controller`.
pub(crate) fn holds(state: &GameState, cond: &Condition, source_controller: PlayerId) -> bool {
    holds_for_source(state, cond, source_controller, None)
}

/// As [`holds`], but also carries the source *object* — needed by `ValueExpr`s that read state
/// relative to the source permanent (e.g. Keen-Eyed's `DistinctCardTypesAmongExiledWith`, which
/// counts cards exiled with THIS creature). A static's condition is evaluated through this.
pub(crate) fn holds_for_source(
    state: &GameState,
    cond: &Condition,
    source_controller: PlayerId,
    source: Option<ObjId>,
) -> bool {
    match cond {
        Condition::Always => true,
        Condition::Not(c) => !holds_for_source(state, c, source_controller, source),
        Condition::All(cs) => cs.iter().all(|c| holds_for_source(state, c, source_controller, source)),
        Condition::AnyOf(cs) => cs.iter().any(|c| holds_for_source(state, c, source_controller, source)),
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
            count >= eval_value(state, n, source_controller, source)
        }
        Condition::LifeAtLeast { who, n } => {
            let p = resolve_player(state, *who, source_controller);
            life(state, p) >= eval_value(state, n, source_controller, source)
        }
        Condition::LifeAtMost { who, n } => {
            let p = resolve_player(state, *who, source_controller);
            life(state, p) <= eval_value(state, n, source_controller, source)
        }
        Condition::GainedLifeThisTurn { who } => {
            let p = resolve_player(state, *who, source_controller);
            state
                .players
                .get(p.0 as usize)
                .map(|pl| pl.life_gained_this_turn > 0)
                .unwrap_or(false)
        }
        Condition::CardLeftGraveyardThisTurn { who } => {
            let p = resolve_player(state, *who, source_controller);
            state
                .players
                .get(p.0 as usize)
                .map(|pl| pl.cards_left_graveyard_this_turn > 0)
                .unwrap_or(false)
        }
        Condition::CreatureDiedThisTurn { who } => {
            let p = resolve_player(state, *who, source_controller);
            state
                .players
                .get(p.0 as usize)
                .map(|pl| pl.creatures_died_this_turn > 0)
                .unwrap_or(false)
        }
        // "you put a counter on this creature this turn" — reads the source permanent's flag.
        Condition::PutCounterOnSelfThisTurn => {
            source.and_then(|s| state.objects.get(&s)).is_some_and(|o| o.counter_added_this_turn)
        }
        // "you've cast an instant or sorcery spell this turn" (Potioner's Trove).
        Condition::CastInstantOrSorceryThisTurn { who } => {
            let p = resolve_player(state, *who, source_controller);
            state
                .players
                .get(p.0 as usize)
                .is_some_and(|pl| pl.instants_sorceries_cast_this_turn > 0)
        }
        Condition::ValueAtLeast(a, b) => {
            eval_value(state, a, source_controller, source)
                >= eval_value(state, b, source_controller, source)
        }
        // "cast from anywhere other than your hand" — the source spell carries `flashback_cast`
        // (the only non-hand cast the engine tracks today). `false` if there's no source object.
        Condition::CastFromNotHand => {
            source.and_then(|s| state.objects.get(&s)).is_some_and(|o| o.flashback_cast)
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

/// Minimal `ValueExpr` eval for conditions: the fixed/count/sum subset plus the source-relative
/// `DistinctCardTypesAmongExiledWith`. Other variants read as 0.
fn eval_value(
    state: &GameState,
    v: &ValueExpr,
    source_controller: PlayerId,
    source: Option<ObjId>,
) -> i64 {
    match v {
        ValueExpr::Fixed(n) => *n,
        ValueExpr::Sum(a, b) => {
            eval_value(state, a, source_controller, source)
                + eval_value(state, b, source_controller, source)
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
        ValueExpr::DistinctCardTypesAmongExiledWith => {
            distinct_card_types_among_exiled_with(state, source)
        }
        // Life `who` gained this turn (Scheming Silvertongue's "if you gained 2+ life this turn").
        ValueExpr::LifeGainedThisTurn { who } => {
            let p = resolve_player(state, *who, source_controller);
            state.players.get(p.0 as usize).map(|pl| pl.life_gained_this_turn as i64).unwrap_or(0)
        }
        // Life-gain events `who` had this turn (Leech Collector's "first time each turn" queue-gate).
        ValueExpr::LifeGainEventsThisTurn { who } => {
            let p = resolve_player(state, *who, source_controller);
            state.players.get(p.0 as usize).map(|pl| pl.life_gain_events_this_turn as i64).unwrap_or(0)
        }
        // Creatures that died this turn, any controller (Emeritus of Woe's "if two or more died").
        ValueExpr::CreaturesDiedThisTurn => {
            state.players.iter().map(|pl| pl.creatures_died_this_turn as i64).sum()
        }
        // Cards put into exile this turn, any owner (Ennis's "if one or more cards … exiled").
        ValueExpr::CardsExiledThisTurn => {
            state.players.iter().map(|pl| pl.cards_exiled_this_turn as i64).sum()
        }
        // Cards in `who`'s hand (Joined Researchers' "an opponent has more cards in hand than you").
        ValueExpr::HandSize { who } => {
            let p = resolve_player(state, *who, source_controller);
            state.players.get(p.0 as usize).map(|pl| pl.hand.len() as i64).unwrap_or(0)
        }
        // Spells `who` cast this turn (Emeritus of Conflict's "your third spell each turn").
        ValueExpr::SpellsCastThisTurn { who } => {
            let p = resolve_player(state, *who, source_controller);
            state.players.get(p.0 as usize).map(|pl| pl.spells_cast_this_turn as i64).unwrap_or(0)
        }
        // Counters on the source object — for an intervening-"if" like "if it has four or more
        // quest counters on it" (Earthbender Ascension). Live count while on the battlefield;
        // otherwise the last-known counter bag (CR 603.10a) so a dies-trigger "if it had one or more
        // counters" (Ambitious Augmenter) reads the count it had at death, not the fresh-object 0.
        ValueExpr::CountersOnSelf(kind) => source
            .map(|s| match state.objects.get(&s) {
                Some(o) if o.zone == Zone::Battlefield => o.counters.get(kind) as i64,
                _ => state.last_known.get(&s).map(|l| l.counters.get(kind)).unwrap_or(0) as i64,
            })
            .unwrap_or(0),
        // Total toughness of matching battlefield permanents (base chars, per this module's model) —
        // Orysa's "creatures you control have total toughness 10 or greater" cost-reduction gate.
        ValueExpr::TotalToughness { filter, controller } => {
            let want = controller.map(|r| resolve_player(state, r, source_controller));
            state
                .objects
                .values()
                .filter(|o| o.zone == Zone::Battlefield)
                .filter(|o| want.is_none_or(|p| o.controller == p))
                .filter(|o| filter_matches(&o.chars, filter))
                .map(|o| o.chars.toughness.unwrap_or(0) as i64)
                .sum()
        }
        _ => 0,
    }
}

/// Count distinct card types among the cards in exile linked to `source` (CR — Keen-Eyed Curator's
/// exile-association). `0` with no source.
pub(crate) fn distinct_card_types_among_exiled_with(state: &GameState, source: Option<ObjId>) -> i64 {
    let Some(src) = source else { return 0 };
    let mut types: BTreeSet<CardType> = BTreeSet::new();
    for o in state.objects.values() {
        if o.zone == Zone::Exile && o.exiled_with == Some(src) {
            types.extend(o.chars.card_types.iter().copied());
        }
    }
    types.len() as i64
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
