//! Readable text renders of game state — used by the interactive CLI (`cli`) and the terminal
//! `HumanAgent`. Two views:
//!
//! - [`render_view`] — one seat's information-filtered [`PlayerView`] (what that player may see:
//!   own hand, public zones, opponents' hands as counts). This is what an interactive player
//!   sees before each decision.
//! - [`render_state`] — a god's-eye dump of the full [`GameState`] (every zone face-up). For
//!   scenario debugging / `show` with no seat argument; not something a player would see.
//!
//! Both return a `String` so the CLI can print it, capture it for scriptable transcripts, or
//! snapshot it with `expect-test`.

use std::fmt::Write as _;

use mtg_core::agent::{ObjView, PlayerView, StopStateView};
use mtg_core::basics::Phase;
use mtg_core::ids::ObjId;
use mtg_core::state::GameState;

/// One seat's view (hidden info already masked by `view_for`).
pub fn render_view(view: &PlayerView) -> String {
    let mut s = String::new();
    let pp = view
        .priority_player
        .map(|p| format!("P{}", p.0))
        .unwrap_or_else(|| "—".into());
    let _ = writeln!(
        s,
        "Turn {} · {:?} · active P{} · priority {}",
        view.turn, view.phase, view.active_player.0, pp
    );
    if let Some(st) = &view.stops {
        let _ = writeln!(s, "  {}", stops_line(st));
    }
    for p in &view.players {
        let you = if p.player == view.seat { " (you)" } else { "" };
        let _ = writeln!(
            s,
            "  P{}{}: life {}  hand {}  library {}  graveyard {}",
            p.player.0,
            you,
            p.life,
            p.hand_count,
            p.library_count,
            p.graveyard.len()
        );
        if !p.graveyard.is_empty() {
            let _ = writeln!(s, "      graveyard: {}", join_objviews(&p.graveyard));
        }
    }
    let bf = join_objviews(&view.battlefield);
    let _ = writeln!(s, "  Battlefield: {}", or_empty(&bf));
    if !view.stack.is_empty() {
        let items: Vec<String> = view.stack.iter().map(|o| o.chars.name.clone()).collect();
        let _ = writeln!(s, "  Stack (top last): {}", items.join(", "));
    }
    let hand = join_objviews(&view.me.hand);
    let _ = write!(s, "  Your hand: {}", or_empty(&hand));
    s
}

/// God's-eye dump of the whole game (every zone face-up). Debugging only.
pub fn render_state(state: &GameState) -> String {
    let mut s = String::new();
    let winner = state
        .winner
        .map(|p| format!("P{}", p.0))
        .unwrap_or_else(|| "—".into());
    let _ = writeln!(
        s,
        "=== Turn {} · {:?} · active P{} · stack {} · game_over {} · winner {} ===",
        state.turn_number,
        state.phase,
        state.active_player.0,
        state.stack.len(),
        state.game_over,
        winner
    );
    for p in &state.players {
        let lost = if p.has_lost { "  [LOST]" } else { "" };
        let _ = writeln!(
            s,
            "P{}: life {} · poison {} · lands_played {}{}",
            p.id.0, p.life, p.poison, p.lands_played_this_turn, lost
        );
        let _ = writeln!(s, "    Hand       ({}): {}", p.hand.len(), names(state, &p.hand));
        let _ = writeln!(
            s,
            "    Library    ({}): {}",
            p.library.len(),
            names(state, &p.library)
        );
        let _ = writeln!(
            s,
            "    Battlefield({}): {}",
            p.battlefield.len(),
            names(state, &p.battlefield)
        );
        if !p.graveyard.is_empty() {
            let _ = writeln!(
                s,
                "    Graveyard  ({}): {}",
                p.graveyard.len(),
                names(state, &p.graveyard)
            );
        }
        if !p.exile.is_empty() {
            let _ = writeln!(s, "    Exile      ({}): {}", p.exile.len(), names(state, &p.exile));
        }
    }
    if !state.stack.is_empty() {
        let items: Vec<String> = state
            .stack
            .items
            .iter()
            .map(|o| match o.kind {
                mtg_core::stack::StackObjectKind::Spell(id) => name_of(state, id),
                mtg_core::stack::StackObjectKind::Ability { .. } => "<ability>".into(),
                mtg_core::stack::StackObjectKind::DelayedAbility { .. } => "<delayed ability>".into(),
                mtg_core::stack::StackObjectKind::ReflexiveAbility { .. } => "<reflexive ability>".into(),
                mtg_core::stack::StackObjectKind::SpellCopyTrigger { .. } => "<copy spell>".into(),
                mtg_core::stack::StackObjectKind::MiracleWindow { .. } => "<miracle>".into(),
            })
            .collect();
        let _ = write!(s, "Stack (top last): {}", items.join(", "));
    }
    s.trim_end().to_string()
}

// ── helpers ─────────────────────────────────────────────────────────────────────────────────

/// A one-line summary of the seat's active priority stops (MTGA-style), for the CLI view.
fn stops_line(st: &StopStateView) -> String {
    if st.full_control {
        return "stops: full control".to_string();
    }
    let active: Vec<String> = st
        .per_step
        .iter()
        .filter(|(_, on)| *on)
        .map(|(p, _)| abbr_step(*p))
        .collect();
    format!(
        "stops: {}",
        if active.is_empty() { "—".to_string() } else { active.join(", ") }
    )
}

fn abbr_step(p: Phase) -> String {
    match p {
        Phase::PrecombatMain => "MP1",
        Phase::PostcombatMain => "MP2",
        Phase::DeclareAttackers => "ATK",
        Phase::DeclareBlockers => "BLK",
        Phase::Upkeep => "UP",
        Phase::Draw => "DR",
        Phase::BeginCombat => "BC",
        Phase::CombatDamage => "CD",
        Phase::EndCombat => "EC",
        Phase::End => "END",
        Phase::Untap => "UN",
        Phase::Cleanup => "CL",
    }
    .to_string()
}

fn or_empty(s: &str) -> String {
    if s.is_empty() {
        "(empty)".into()
    } else {
        s.to_string()
    }
}

fn name_of(state: &GameState, id: ObjId) -> String {
    state
        .objects
        .get(&id)
        .map(|o| {
            if o.status.tapped {
                format!("{} (tapped)", o.chars.name)
            } else {
                o.chars.name.clone()
            }
        })
        .unwrap_or_else(|| format!("#{}", id.0))
}

fn names(state: &GameState, ids: &[ObjId]) -> String {
    if ids.is_empty() {
        return "(empty)".into();
    }
    ids.iter()
        .map(|&id| name_of(state, id))
        .collect::<Vec<_>>()
        .join(", ")
}

fn join_objviews(objs: &[ObjView]) -> String {
    objs.iter()
        .map(|o| match o {
            ObjView::Visible { chars, status, .. } => {
                // Computed (post-layer) characteristics: P/T includes anthems/counters, keywords
                // include layer-granted ones (e.g. Flying from Levitation).
                let mut s = chars.name.clone();
                if let (Some(p), Some(t)) = (chars.power, chars.toughness) {
                    s += &format!(" {p}/{t}");
                }
                if !chars.keywords.is_empty() {
                    s += &format!(" [{}]", chars.keywords.join(", "));
                }
                if status.tapped {
                    s += " (tapped)";
                }
                s
            }
            ObjView::Hidden { .. } => "(hidden)".into(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}
