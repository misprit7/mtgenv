//! M1 — [`HumanAgent`]: a human at the terminal is **just another [`Agent`]**.
//!
//! It implements the exact same `decide()` the scripted AI and the future RL policy implement:
//! the engine has already enumerated the legal options, so this backend only prints them and
//! reads a chosen index from stdin. There is no rules logic here — masking is the engine's job
//! (CLIENT_PLAN §1/§3). If stdin parsing fails or hits EOF, it falls back to a safe default
//! (pass / no selection) so a piped or closed terminal can't wedge the game.

use std::io::{self, Write};

use mtg_core::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, ObjView, PlayerView};
use mtg_core::ids::PlayerId;

use crate::options::{self, Mode, Selection};

/// A terminal-driven [`Agent`] for one seat.
pub struct HumanAgent {
    pub seat: PlayerId,
}

impl HumanAgent {
    pub fn new(seat: PlayerId) -> Self {
        HumanAgent { seat }
    }
}

impl Agent for HumanAgent {
    fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
        print_view(view);
        let prompt = options::prompt_for(view, req);
        println!("\n── DECISION ──────────────────────────────────────────────");
        println!("{}", prompt.title);
        for (i, o) in prompt.options.iter().enumerate() {
            println!("  [{i}] {o}");
        }
        let sel = read_selection(&prompt);
        options::response_from(req, &sel)
    }

    fn observe(&mut self, _view: &PlayerView, ev: &GameEvent) {
        println!("  · {ev:?}");
    }
}

/// Read a [`Selection`] from stdin according to the prompt's input `mode`.
fn read_selection(prompt: &options::Prompt) -> Selection {
    match prompt.mode {
        Mode::Action => {
            if prompt.can_pass {
                print!("Choose action index, or [Enter]/'p' to pass: ");
            } else {
                print!("Choose action index: ");
            }
            let line = read_line();
            let t = line.trim();
            if t.is_empty() || t.eq_ignore_ascii_case("p") {
                Selection {
                    pass: true,
                    ..Default::default()
                }
            } else if let Ok(i) = t.parse::<u32>() {
                Selection {
                    picks: vec![i.min(prompt.options.len().saturating_sub(1) as u32)],
                    ..Default::default()
                }
            } else {
                Selection {
                    pass: true,
                    ..Default::default()
                }
            }
        }
        Mode::SelectOne => {
            print!("Choose one index [0..{}]: ", prompt.options.len().saturating_sub(1));
            let i = read_line().trim().parse::<u32>().unwrap_or(0);
            Selection {
                picks: vec![i.min(prompt.options.len().saturating_sub(1) as u32)],
                ..Default::default()
            }
        }
        Mode::SelectMany => {
            print!(
                "Choose {}..{} indices (space-separated, blank = none): ",
                prompt.min, prompt.max
            );
            let line = read_line();
            let picks: Vec<u32> = line
                .split_whitespace()
                .filter_map(|s| s.parse::<u32>().ok())
                .filter(|&i| (i as usize) < prompt.options.len())
                .collect();
            Selection {
                picks,
                ..Default::default()
            }
        }
        Mode::Number => {
            print!("Enter a number in [{}, {}]: ", prompt.num_min, prompt.num_max);
            let n = read_line()
                .trim()
                .parse::<i64>()
                .unwrap_or(prompt.num_min)
                .clamp(prompt.num_min, prompt.num_max);
            Selection {
                number: Some(n),
                ..Default::default()
            }
        }
        Mode::Order => {
            print!(
                "Enter an ordering as space-separated indices (blank = keep order): "
            );
            let line = read_line();
            let order: Vec<u32> = line
                .split_whitespace()
                .filter_map(|s| s.parse::<u32>().ok())
                .collect();
            Selection {
                order,
                ..Default::default()
            }
        }
    }
}

fn read_line() -> String {
    io::stdout().flush().ok();
    let mut s = String::new();
    // EOF (piped/closed stdin) → empty string → safe defaults above.
    io::stdin().read_line(&mut s).ok();
    s
}

/// A compact textual render of the seat's view (board + hand), so the terminal player has
/// context for the decision.
fn print_view(view: &PlayerView) {
    println!("\n══════════════════════════════════════════════════════════");
    println!(
        "Turn {} · {:?} · active = Player {} · priority = {}",
        view.turn,
        view.phase,
        view.active_player.0,
        view.priority_player
            .map(|p| p.0.to_string())
            .unwrap_or_else(|| "—".into()),
    );
    for p in &view.players {
        let you = if p.player == view.seat { " (you)" } else { "" };
        println!(
            "  Player {}{}: life {}  hand {}  library {}  graveyard {}",
            p.player.0,
            you,
            p.life,
            p.hand_count,
            p.library_count,
            p.graveyard.len(),
        );
    }
    let bf = render_objs(&view.battlefield);
    if !bf.is_empty() {
        println!("  Battlefield: {bf}");
    }
    if !view.stack.is_empty() {
        let s: Vec<String> = view.stack.iter().map(|o| o.chars.name.clone()).collect();
        println!("  Stack: {}", s.join(", "));
    }
    let hand = render_objs(&view.me.hand);
    println!("  Your hand: {}", if hand.is_empty() { "(empty)".into() } else { hand });
}

fn render_objs(objs: &[ObjView]) -> String {
    let names: Vec<String> = objs
        .iter()
        .map(|o| match o {
            ObjView::Visible { chars, status, .. } => {
                if status.tapped {
                    format!("{} (tapped)", chars.name)
                } else {
                    chars.name.clone()
                }
            }
            ObjView::Hidden { .. } => "(hidden)".into(),
        })
        .collect();
    names.join(", ")
}
