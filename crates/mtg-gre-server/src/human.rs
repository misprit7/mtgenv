//! M1 — [`HumanAgent`]: a human at the terminal is **just another [`Agent`]**.
//!
//! It implements the exact same `decide()` the scripted AI and the future RL policy implement:
//! the engine has already enumerated the legal options, so this backend only renders them and
//! reads a chosen index. No rules logic — masking is the engine's job (CLIENT_PLAN §1/§3).
//!
//! Input/output flow through a shared [`SharedIo`](crate::cli::SharedIo) so the same agent drives
//! both an interactive terminal session and a scripted run (the CLI feeds setup commands and the
//! player's decision lines from one stream). At a decision prompt the player may type a
//! meta-command (`?`/`dump`) to re-inspect their view without consuming the decision. On EOF
//! (closed/piped input) it falls back to a safe default so a closed terminal can't wedge the game.

use mtg_core::agent::{Agent, DecisionRequest, DecisionResponse, GameEvent, PlayerView};
use mtg_core::ids::PlayerId;

use crate::cli::SharedIo;
use crate::options::{self, Mode, Prompt};
use crate::render;

const DECISION_HELP: &str =
    "  commands: <index> select · (multi: space-separated) · p/Enter pass · ? help · dump re-show view";

/// A terminal-driven [`Agent`] for one seat, reading/writing through a shared IO handle.
pub struct HumanAgent {
    pub seat: PlayerId,
    io: SharedIo,
}

impl HumanAgent {
    pub fn new(seat: PlayerId, io: SharedIo) -> Self {
        HumanAgent { seat, io }
    }
}

impl Agent for HumanAgent {
    fn decide(&mut self, view: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
        let prompt = options::prompt_for(view, req);
        {
            let mut io = self.io.borrow_mut();
            io.say("");
            io.say(&render::render_view(view));
            io.say(&format!("── DECISION (P{}): {}", self.seat.0, prompt.title));
            for (i, o) in prompt.options.iter().enumerate() {
                io.say(&format!("    [{i}] {o}"));
            }
        }
        loop {
            self.io.borrow_mut().print(&hint(&prompt));
            let line = self.io.borrow_mut().next_line();
            let line = match line {
                Some(l) => l,
                None => return options::default_response(req), // EOF
            };
            match line.trim() {
                "?" | "help" => self.io.borrow_mut().say(DECISION_HELP),
                "dump" | "view" | "state" => {
                    let r = render::render_view(view);
                    self.io.borrow_mut().say(&r);
                }
                other => return options::response_from(req, &options::parse_selection(&prompt, other)),
            }
        }
    }

    fn observe(&mut self, _view: &PlayerView, ev: &GameEvent) {
        self.io.borrow_mut().say(&format!("  · {ev:?}"));
    }
}

/// The one-line input hint shown for a prompt's input mode.
fn hint(prompt: &Prompt) -> String {
    match prompt.mode {
        Mode::Action => "  action index, or [Enter]/p to pass > ".into(),
        Mode::SelectOne => format!("  choose one [0..{}] > ", prompt.options.len().saturating_sub(1)),
        Mode::SelectMany => format!("  choose {}..{} indices (space-separated) > ", prompt.min, prompt.max),
        Mode::Number => format!("  enter a number [{}..{}] > ", prompt.num_min, prompt.num_max),
        Mode::Order => "  enter an ordering (space-separated indices) > ".into(),
    }
}
