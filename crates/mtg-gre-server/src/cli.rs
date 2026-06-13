//! The expressive interactive / scriptable CLI over the `mtg-core` Agent boundary.
//!
//! It is a small command interpreter that lets a human (or a script, or another agent) **set up
//! scenarios**, **inspect state**, and **play** — all against the real engine:
//!
//! - scenario setup: `new`, `life`, `add` (place cards in a zone), `deck`, `handsize`, `seat`;
//! - inspection: `show` (god view, or a seat's information-filtered `PlayerView`);
//! - play: `run`/`play` — run the game through [`mtg_core::priority::Engine`] with a
//!   [`HumanAgent`](crate::human::HumanAgent) for human seats and a `RandomAgent` otherwise.
//!
//! Input and output flow through one [`SharedIo`] handle that the `HumanAgent`s also use, so a
//! single stream carries setup commands *and* in-game decisions. With `--script file` the whole
//! session is deterministic (seeded RNG) and the transcript is stable — ideal for `expect-test`
//! scenario snapshots.

use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::rc::Rc;

use mtg_core::agent::{Agent, RandomAgent};
use mtg_core::basics::Zone;
use mtg_core::ids::PlayerId;
use mtg_core::priority::Engine;
use mtg_core::state::view::view_for;
use mtg_core::state::{Characteristics, GameState};

use crate::human::HumanAgent;
use crate::render;

/// A shared, line-oriented input/output handle. Single-threaded (the CLI and the engine run on
/// one thread), so `Rc<RefCell<…>>` is the right sharing primitive — the `HumanAgent`s hold
/// clones of the same handle.
pub type SharedIo = Rc<RefCell<CliIo>>;

/// Line-buffered IO with optional input echo (used to interleave scripted commands into the
/// transcript so a captured run reads top-to-bottom).
pub struct CliIo {
    reader: Box<dyn BufRead>,
    out: Box<dyn Write>,
    echo: bool,
}

impl CliIo {
    pub fn new(reader: Box<dyn BufRead>, out: Box<dyn Write>, echo: bool) -> SharedIo {
        Rc::new(RefCell::new(CliIo { reader, out, echo }))
    }

    /// Read one line (newline stripped). `None` at EOF. Echoes the line to output when `echo`
    /// is set (scripted mode), so the transcript shows what was "typed".
    pub fn next_line(&mut self) -> Option<String> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                let line = line.trim_end_matches(['\n', '\r']).to_string();
                if self.echo {
                    let _ = writeln!(self.out, "{line}");
                }
                Some(line)
            }
        }
    }

    pub fn say(&mut self, s: &str) {
        let _ = writeln!(self.out, "{s}");
    }

    pub fn print(&mut self, s: &str) {
        let _ = write!(self.out, "{s}");
        let _ = self.out.flush();
    }
}

/// What kind of agent occupies a seat.
enum SeatSpec {
    Human,
    Random(u64),
}

enum Flow {
    Continue,
    Quit,
}

/// The five basic lands, used for round-robin deck building.
const BASICS: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];

/// CLI session state: the scenario under construction + seat assignment.
pub struct Cli {
    io: SharedIo,
    state: GameState,
    seats: Vec<SeatSpec>,
    deal: bool,
    seed: u64,
}

/// Run the CLI REPL against `io` until EOF or `quit`.
pub fn run(io: SharedIo) {
    let mut cli = Cli::new(io);
    cli.banner();
    loop {
        cli.print("mtg> ");
        let line = match cli.io.borrow_mut().next_line() {
            Some(l) => l,
            None => break,
        };
        let line = line.trim().to_string();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Flow::Quit = cli.dispatch(&line) {
            break;
        }
    }
}

impl Cli {
    fn new(io: SharedIo) -> Self {
        let seed = 0;
        Cli {
            io,
            state: GameState::new(2, seed),
            seats: vec![SeatSpec::Human, SeatSpec::Random(seed ^ 0xB0B)],
            deal: true,
            seed,
        }
    }

    fn say(&self, s: &str) {
        self.io.borrow_mut().say(s);
    }
    fn print(&self, s: &str) {
        self.io.borrow_mut().print(s);
    }

    fn banner(&self) {
        self.say("mtgenv CLI — interactive play + scenario setup over the Agent boundary.");
        self.say("Type 'help' for commands, or 'play' to start a lands-only game vs RandomAgent.");
    }

    fn dispatch(&mut self, line: &str) -> Flow {
        let mut parts = line.split_whitespace();
        let cmd = parts.next().unwrap_or("");
        let args: Vec<&str> = parts.collect();
        match cmd {
            "help" | "h" => self.help(),
            "new" => self.cmd_new(&args),
            "seat" => self.cmd_seat(&args),
            "life" => self.cmd_life(&args),
            "add" => self.cmd_add(&args),
            "deck" => self.cmd_deck(&args),
            "handsize" => self.cmd_handsize(&args),
            "deal" => self.cmd_deal(&args),
            "show" | "dump" => self.cmd_show(&args),
            "play" => self.cmd_play(&args),
            "run" | "start" => self.cmd_run(),
            "quit" | "exit" | "q" => return Flow::Quit,
            other => self.say(&format!("unknown command: {other} (try 'help')")),
        }
        Flow::Continue
    }

    fn help(&self) {
        let h = "\
Commands:
  new [players] [seed]        fresh game (default 2 players); resets seats (P0 human, rest random)
  seat <i> human|random[:sd]  set seat i's agent
  life <player> <n>           set a player's life total
  add <player> <zone> <card>… place card(s) in a zone (zone: library|hand|battlefield|graveyard|exile)
                              card: a basic land name (Plains|Island|Swamp|Mountain|Forest)
  deck <player> <count> [name] add <count> basics to the library (round-robin, or all <name>)
  handsize <player> <n>       set maximum hand size
  deal on|off                 deal opening hands on 'run' (default on)
  show [player]               dump full state (no arg) or a seat's PlayerView
  play [lands] [seed]         quick game vs RandomAgent (demo deck = creatures+burn; 'lands' = lands-only)
  run                         run the configured scenario to completion
  quit                        exit
Lines starting with '#' are comments. At a decision prompt: an index, 'p'/Enter to pass, '?' help, 'dump' to re-show.";
        self.say(h);
    }

    // ── scenario setup ─────────────────────────────────────────────────────────────────────

    fn cmd_new(&mut self, args: &[&str]) {
        let players = args.first().and_then(|s| s.parse().ok()).unwrap_or(2usize).max(1);
        let seed = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(self.seed);
        self.seed = seed;
        self.state = GameState::new(players, seed);
        self.seats = (0..players)
            .map(|i| if i == 0 { SeatSpec::Human } else { SeatSpec::Random(seed ^ (0xB0B + i as u64)) })
            .collect();
        self.say(&format!("new game: {players} players, seed {seed} (P0 human, rest random)"));
    }

    fn cmd_seat(&mut self, args: &[&str]) {
        let Some(i) = args.first().and_then(|s| s.parse::<usize>().ok()) else {
            return self.say("usage: seat <i> human|random[:seed]");
        };
        if i >= self.seats.len() {
            return self.say(&format!("no seat {i} (game has {} seats)", self.seats.len()));
        }
        let spec = args.get(1).copied().unwrap_or("");
        let seat = if spec.eq_ignore_ascii_case("human") {
            SeatSpec::Human
        } else if let Some(rest) = spec.strip_prefix("random") {
            let sd = rest.strip_prefix(':').and_then(|s| s.parse().ok()).unwrap_or(self.seed ^ i as u64);
            SeatSpec::Random(sd)
        } else {
            return self.say("seat type must be 'human' or 'random[:seed]'");
        };
        self.seats[i] = seat;
        self.say(&format!("seat {i} set"));
    }

    fn cmd_life(&mut self, args: &[&str]) {
        let (Some(p), Some(n)) = (
            args.first().and_then(|s| s.parse::<u32>().ok()),
            args.get(1).and_then(|s| s.parse::<i32>().ok()),
        ) else {
            return self.say("usage: life <player> <n>");
        };
        if !self.valid_player(p) {
            return;
        }
        self.state.player_mut(PlayerId(p)).life = n;
        self.say(&format!("P{p} life = {n}"));
    }

    fn cmd_add(&mut self, args: &[&str]) {
        if args.len() < 3 {
            return self.say("usage: add <player> <zone> <card>…");
        }
        let Some(p) = args[0].parse::<u32>().ok().filter(|&p| self.valid_player(p)) else {
            return;
        };
        let Some(zone) = parse_zone(args[1]) else {
            return self.say(&format!("unknown zone '{}' (library|hand|battlefield|graveyard|exile)", args[1]));
        };
        let mut added = 0;
        for spec in &args[2..] {
            match parse_card(spec) {
                Some(chars) => {
                    self.state.add_card(PlayerId(p), chars, zone);
                    added += 1;
                }
                None => self.say(&format!("unknown card '{spec}' (basic land names only for now)")),
            }
        }
        self.say(&format!("added {added} card(s) to P{p}'s {zone:?}"));
    }

    fn cmd_deck(&mut self, args: &[&str]) {
        let (Some(p), Some(count)) = (
            args.first().and_then(|s| s.parse::<u32>().ok()),
            args.get(1).and_then(|s| s.parse::<usize>().ok()),
        ) else {
            return self.say("usage: deck <player> <count> [land-name]");
        };
        if !self.valid_player(p) {
            return;
        }
        let fixed = args.get(2).and_then(|s| parse_card(s));
        for i in 0..count {
            let chars = fixed
                .clone()
                .unwrap_or_else(|| Characteristics::basic_land(BASICS[i % BASICS.len()]));
            self.state.add_card(PlayerId(p), chars, Zone::Library);
        }
        self.say(&format!("added {count} land(s) to P{p}'s library"));
    }

    fn cmd_handsize(&mut self, args: &[&str]) {
        let (Some(p), Some(n)) = (
            args.first().and_then(|s| s.parse::<u32>().ok()),
            args.get(1).and_then(|s| s.parse::<usize>().ok()),
        ) else {
            return self.say("usage: handsize <player> <n>");
        };
        if !self.valid_player(p) {
            return;
        }
        self.state.player_mut(PlayerId(p)).hand_size_limit = n;
        self.say(&format!("P{p} hand size = {n}"));
    }

    fn cmd_deal(&mut self, args: &[&str]) {
        match args.first().copied() {
            Some("on") => {
                self.deal = true;
                self.say("opening deal: on");
            }
            Some("off") => {
                self.deal = false;
                self.say("opening deal: off");
            }
            _ => self.say("usage: deal on|off"),
        }
    }

    // ── inspection ──────────────────────────────────────────────────────────────────────────

    fn cmd_show(&self, args: &[&str]) {
        match args.first().and_then(|s| s.parse::<u32>().ok()) {
            Some(p) if (p as usize) < self.state.players.len() => {
                let view = view_for(&self.state, PlayerId(p));
                self.say(&render::render_view(&view));
            }
            Some(p) => self.say(&format!("no player {p}")),
            None => self.say(&render::render_state(&self.state)),
        }
    }

    // ── play ─────────────────────────────────────────────────────────────────────────────────

    fn cmd_play(&mut self, args: &[&str]) {
        // `play` → demo game (lands + creatures + burn); `play lands` → lands-only.
        let lands = args.first() == Some(&"lands");
        let rest: &[&str] = if lands { &args[1..] } else { args };
        let seed = rest.first().and_then(|s| s.parse().ok()).unwrap_or(self.seed);
        self.seed = seed;
        self.state = if lands {
            crate::driver::lands_only_state(2, seed)
        } else {
            crate::driver::demo_state(seed)
        };
        self.seats = vec![SeatSpec::Human, SeatSpec::Random(seed ^ 0xB0B)];
        self.deal = true;
        self.cmd_run();
    }

    fn cmd_run(&mut self) {
        // One agent per seat, in PlayerId order.
        if self.seats.len() != self.state.players.len() {
            self.seats = (0..self.state.players.len())
                .map(|i| if i == 0 { SeatSpec::Human } else { SeatSpec::Random(self.seed ^ i as u64) })
                .collect();
        }
        let agents: Vec<Box<dyn Agent>> = self
            .seats
            .iter()
            .enumerate()
            .map(|(i, spec)| match spec {
                SeatSpec::Human => {
                    Box::new(HumanAgent::new(PlayerId(i as u32), self.io.clone())) as Box<dyn Agent>
                }
                SeatSpec::Random(seed) => Box::new(RandomAgent::new(*seed)) as Box<dyn Agent>,
            })
            .collect();

        if !self.deal {
            // Running a scenario with exact hands (no opening draw) needs an engine hook to skip
            // the deal; it isn't exposed yet, so note it and run with the deal for now.
            self.say("(note: 'deal off' needs an engine hook to skip the opening draw — pending; running WITH deal)");
        }

        let mut engine = Engine::new(self.state.clone(), agents);
        let winner = engine.run_game();
        // Keep the finished state so post-game `show` works.
        self.state = engine.state;
        let w = winner.map(|p| format!("P{}", p.0)).unwrap_or_else(|| "draw".into());
        self.say(&format!("\n═══ GAME OVER — winner {w} (turn {}) ═══", self.state.turn_number));
    }

    // ── helpers ──────────────────────────────────────────────────────────────────────────────

    fn valid_player(&self, p: u32) -> bool {
        if (p as usize) < self.state.players.len() {
            true
        } else {
            self.say(&format!("no player {p} (game has {} players)", self.state.players.len()));
            false
        }
    }
}

fn parse_zone(s: &str) -> Option<Zone> {
    match s.to_ascii_lowercase().as_str() {
        "library" | "lib" | "deck" => Some(Zone::Library),
        "hand" => Some(Zone::Hand),
        "battlefield" | "bf" | "board" | "play" => Some(Zone::Battlefield),
        "graveyard" | "gy" | "grave" => Some(Zone::Graveyard),
        "exile" => Some(Zone::Exile),
        _ => None,
    }
}

/// Parse a card spec. For now only basic land names (case-insensitive); creature/spell specs
/// arrive once engine #9 settles the card-type vocabulary.
fn parse_card(spec: &str) -> Option<Characteristics> {
    let name = match spec.to_ascii_lowercase().as_str() {
        "plains" => "Plains",
        "island" => "Island",
        "swamp" => "Swamp",
        "mountain" => "Mountain",
        "forest" => "Forest",
        _ => return None,
    };
    Some(Characteristics::basic_land(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    /// Drive the CLI with a script and capture the full transcript (deterministic: seeded RNG,
    /// echoed input). This doubles as a scenario test + living documentation of the interface.
    fn run_script(script: &str) -> String {
        let reader: Box<dyn BufRead> = Box::new(std::io::Cursor::new(script.to_string()));
        let buf = Rc::new(RefCell::new(Vec::<u8>::new()));
        let out = Box::new(SharedBuf(buf.clone()));
        let io = CliIo::new(reader, out, true);
        run(io);
        let bytes = buf.borrow().clone();
        String::from_utf8(bytes).unwrap()
    }

    /// A `Write` into a shared `Vec<u8>` so the test can read back the transcript.
    struct SharedBuf(Rc<RefCell<Vec<u8>>>);
    impl Write for SharedBuf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn scenario_setup_and_show_render() {
        let transcript = run_script(
            "new 2 1\n\
             life 0 17\n\
             add 0 hand Forest Island\n\
             add 1 battlefield Mountain\n\
             deck 1 3\n\
             show\n\
             quit\n",
        );
        expect![[r#"
            mtgenv CLI — interactive play + scenario setup over the Agent boundary.
            Type 'help' for commands, or 'play' to start a lands-only game vs RandomAgent.
            mtg> new 2 1
            new game: 2 players, seed 1 (P0 human, rest random)
            mtg> life 0 17
            P0 life = 17
            mtg> add 0 hand Forest Island
            added 2 card(s) to P0's Hand
            mtg> add 1 battlefield Mountain
            added 1 card(s) to P1's Battlefield
            mtg> deck 1 3
            added 3 land(s) to P1's library
            mtg> show
            === Turn 1 · Untap · active P0 · stack 0 · game_over false · winner — ===
            P0: life 17 · poison 0 · lands_played 0
                Hand       (2): Forest, Island
                Library    (0): (empty)
                Battlefield(0): (empty)
            P1: life 20 · poison 0 · lands_played 0
                Hand       (0): (empty)
                Library    (3): Plains, Island, Swamp
                Battlefield(1): Mountain
            mtg> quit
        "#]]
        .assert_eq(&transcript);
    }

    #[test]
    fn scripted_game_plays_to_completion() {
        // A short scripted game: the human (P0) passes every decision (blank lines), the bot is
        // random. The transcript ends with a winner — deterministic for the seed.
        let mut script = String::from("play 3\n");
        for _ in 0..400 {
            script.push('\n'); // pass / default every prompt
        }
        script.push_str("quit\n");
        let transcript = run_script(&script);
        assert!(
            transcript.contains("GAME OVER — winner"),
            "scripted game should finish:\n{transcript}"
        );
    }
}
