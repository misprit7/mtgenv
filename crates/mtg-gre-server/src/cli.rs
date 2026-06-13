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
use std::sync::Arc;

use mtg_core::agent::{Agent, RandomAgent};
use mtg_core::basics::{Phase, Zone};
use mtg_core::cards;
use mtg_core::ids::PlayerId;
use mtg_core::priority::Engine;
use mtg_core::sba::LossReason;
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

/// The basic lands present in the starter card DB (round-robin deck building).
const DB_BASICS: [&str; 4] = ["plains", "island", "mountain", "forest"];
/// Help blurb of recognized card aliases for `add`/`deck`.
const CARD_NAMES: &str =
    "plains island mountain forest bears giant shock divination salve bolt visionary kavu servant fogbank anthem levitation humility";

/// CLI session state: the scenario under construction + seat assignment.
pub struct Cli {
    io: SharedIo,
    state: GameState,
    seats: Vec<SeatSpec>,
    deal: bool,
    seed: u64,
    /// MTGA-style auto-pass / stop config for human seats, applied at `run`.
    stops: crate::driver::Stops,
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
            state: fresh_state(2, seed),
            seats: vec![SeatSpec::Human, SeatSpec::Random(seed ^ 0xB0B)],
            deal: true,
            seed,
            stops: crate::driver::Stops::default(),
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
        self.say("Type 'help' for commands, or 'play' to start a game vs RandomAgent (creatures + combat).");
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
            "preset" => self.cmd_preset(&args),
            "handsize" => self.cmd_handsize(&args),
            "deal" => self.cmd_deal(&args),
            "autopass" => self.cmd_autopass(&args),
            "fullcontrol" => self.cmd_fullcontrol(&args),
            "smartstops" => self.cmd_smartstops(&args),
            "resolvestack" => self.cmd_resolvestack(&args),
            "stop" => self.cmd_stop(&args),
            "stops" => self.cmd_stops(),
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
                              cards: plains island mountain forest bears giant shock divination salve bolt
                                     visionary kavu servant fogbank (triggers/counters/prevention)
                                     anthem levitation humility (layers: P/T anthem, keyword grant, Humility)
  deck <player> <count> [name] add <count> cards to the library (round-robin basics, or all <name>)
  preset <seat> <burn|bears|demo>  load a named preset deck into a seat's library
  handsize <player> <n>       set maximum hand size
  deal on|off                 deal opening hands on 'run' (off = play the hand-built scenario as-is)
  autopass on|off             MTGA auto-pass (on, default: prompt only at stops; off: every window)
  smartstops on|off           stop wherever you have a legal play (MTGA default on)
  fullcontrol on|off          stop at every priority window
  resolvestack on|off         auto-pass your own stack objects (MTGA default on)
  stop <step> on|off|default  override a stop (steps: mp1 mp2 upkeep draw attackers blockers …)
  stops                       show the current stop config
  show [player]               dump full state (no arg) or a seat's PlayerView
  play [decks…] [seed]        quick game vs RandomAgent. decks: demo (default)|lands|burn|bears;
                              one deck = both seats, two = P0 P1 (e.g. 'play burn bears')
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
        self.state = fresh_state(players, seed);
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
            match self.card_by_name(spec) {
                Some(chars) => {
                    self.state.add_card(PlayerId(p), chars, zone);
                    added += 1;
                }
                None => self.say(&format!("unknown card '{spec}' (try: {CARD_NAMES})")),
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
        // Optional fixed card name; otherwise round-robin the basic lands in the card DB.
        let fixed = args.get(2).map(|s| s.to_string());
        let mut added = 0;
        for i in 0..count {
            let name = fixed.as_deref().unwrap_or(DB_BASICS[i % DB_BASICS.len()]);
            if let Some(chars) = self.card_by_name(name) {
                self.state.add_card(PlayerId(p), chars, Zone::Library);
                added += 1;
            }
        }
        if added < count {
            self.say(&format!("unknown card name (try: {CARD_NAMES})"));
        }
        self.say(&format!("added {added} card(s) to P{p}'s library"));
    }

    /// Load a named preset deck (`burn`/`bears`/`demo`) into a seat's library — composes with
    /// `new`/`seat`/`add`/`deal`/`run` for building the user's matchups by hand.
    fn cmd_preset(&mut self, args: &[&str]) {
        let (Some(seat), Some(name)) = (
            args.first().and_then(|s| s.parse::<u32>().ok()),
            args.get(1),
        ) else {
            return self.say("usage: preset <seat> <burn|bears|demo>");
        };
        if !self.valid_player(seat) {
            return;
        }
        let Some(deck) = cards::preset_deck(name) else {
            return self.say(&format!("unknown preset '{name}' (burn|bears|demo)"));
        };
        let db = self.state.card_db();
        let chars: Vec<Characteristics> = deck
            .iter()
            .filter_map(|&g| db.get(g).map(|d| d.chars.clone()))
            .collect();
        let n = chars.len();
        for c in chars {
            self.state.add_card(PlayerId(seat), c, Zone::Library);
        }
        self.say(&format!("loaded {n}-card '{name}' deck into P{seat}'s library"));
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

    // ── stops (MTGA-style auto-pass) ─────────────────────────────────────────────────────────

    fn cmd_autopass(&mut self, args: &[&str]) {
        match args.first().copied() {
            Some("on") => { self.stops.auto_pass = true; self.say("auto-pass: on (Arena — prompt only at stops + decisions)"); }
            Some("off") => { self.stops.auto_pass = false; self.say("auto-pass: off (paper CR — prompt at every priority window)"); }
            _ => self.say("usage: autopass on|off"),
        }
    }

    fn cmd_fullcontrol(&mut self, args: &[&str]) {
        match args.first().copied() {
            Some("on") => { self.stops.full_control = true; self.say("full control: on (stop at every priority window)"); }
            Some("off") => { self.stops.full_control = false; self.say("full control: off"); }
            _ => self.say("usage: fullcontrol on|off"),
        }
    }

    fn cmd_smartstops(&mut self, args: &[&str]) {
        match args.first().copied() {
            Some("on") => { self.stops.smart_stops = true; self.say("smart stops: on (stop wherever you have a legal play)"); }
            Some("off") => { self.stops.smart_stops = false; self.say("smart stops: off"); }
            _ => self.say("usage: smartstops on|off"),
        }
    }

    fn cmd_resolvestack(&mut self, args: &[&str]) {
        match args.first().copied() {
            Some("on") => { self.stops.resolve_own_stack = true; self.say("resolve own stack: on (auto-pass while your own object is resolving)"); }
            Some("off") => { self.stops.resolve_own_stack = false; self.say("resolve own stack: off (you may respond to your own spells)"); }
            _ => self.say("usage: resolvestack on|off"),
        }
    }

    fn cmd_stop(&mut self, args: &[&str]) {
        let (Some(step_s), Some(val_s)) = (args.first(), args.get(1)) else {
            return self.say("usage: stop <step> on|off|default  (steps: mp1 mp2 upkeep draw attackers blockers …)");
        };
        let Some(step) = parse_step(step_s) else {
            return self.say(&format!("unknown step '{step_s}' (mp1|mp2|upkeep|draw|attackers|blockers|begincombat|combatdamage|endcombat|end|cleanup|untap)"));
        };
        // The CLI `stop` command toggles BOTH turn sides of a step (the web UI does per-side live).
        self.stops.overrides.retain(|(s, _, _)| *s != step);
        match *val_s {
            "on" | "always" => { self.stops.overrides.extend([(step, true, true), (step, false, true)]); self.say(&format!("stop at {step:?}: always")); }
            "off" | "never" => { self.stops.overrides.extend([(step, true, false), (step, false, false)]); self.say(&format!("stop at {step:?}: never")); }
            "default" => self.say(&format!("stop at {step:?}: Arena default")),
            _ => self.say("usage: stop <step> on|off|default"),
        }
    }

    fn cmd_stops(&self) {
        let s = &self.stops;
        let mut out = String::from("Stops (MTGA profile):\n");
        out += &format!("  auto-pass:     {}\n", if s.auto_pass { "on" } else { "off (paper CR)" });
        out += &format!("  smart stops:   {}\n", if s.smart_stops { "on (stop where you have a play)" } else { "off" });
        out += &format!("  full control:  {}\n", if s.full_control { "on (stop everywhere)" } else { "off" });
        out += &format!("  resolve stack: {}\n", if s.resolve_own_stack { "on (auto-pass your own stack)" } else { "off (respond to self)" });
        out += "  default stops: your main 1 + main 2, opponent's begin-combat + end step\n";
        out += "  (declare-attackers/blockers are always presented as forced decisions)\n";
        if s.overrides.is_empty() {
            out += "  overrides:     (none)";
        } else {
            out.push_str("  overrides:    ");
            for (st, own, v) in &s.overrides {
                out += &format!(" {st:?}@{}={}", if *own { "you" } else { "opp" }, if *v { "always" } else { "never" });
            }
        }
        self.say(&out);
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
        // Args are deck names (non-numeric) plus an optional seed (numeric):
        //   play                → demo deck both seats
        //   play lands           → lands-only
        //   play burn bears 7    → P0 burn, P1 bears, seed 7
        let mut deck_names: Vec<&str> = Vec::new();
        let mut seed = self.seed;
        for a in args {
            match a.parse::<u64>() {
                Ok(n) => seed = n,
                Err(_) => deck_names.push(a),
            }
        }
        let Some(state) = build_play_state(&deck_names, seed) else {
            return self.say("usage: play [demo|lands|burn|bears] [deck1] [seed]  (unknown deck name)");
        };
        self.seed = seed;
        self.state = state;
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

        let mut engine = Engine::new(self.state.clone(), agents);
        if !self.deal {
            // Play the hand-built scenario as-is: no shuffle, no opening draw (engine hook).
            engine.skip_opening_deal();
        }
        // MTGA-style auto-pass / stops for the human seat(s).
        let human_seats: Vec<PlayerId> = self
            .seats
            .iter()
            .enumerate()
            .filter(|(_, s)| matches!(s, SeatSpec::Human))
            .map(|(i, _)| PlayerId(i as u32))
            .collect();
        crate::driver::apply_stops(&mut engine, &self.stops, &human_seats);
        let winner = engine.run_game();
        // Keep the finished state so post-game `show` works.
        self.state = engine.state;
        let w = winner.map(|p| format!("P{}", p.0)).unwrap_or_else(|| "draw".into());
        let reason = match self.state.end_reason {
            Some(LossReason::ZeroOrLessLife) => " — a player hit 0 life",
            Some(LossReason::DrewFromEmptyLibrary) => " — a player decked out",
            Some(LossReason::TenPoison) => " — a player got 10 poison",
            None => "",
        };
        self.say(&format!(
            "\n═══ GAME OVER — winner {w} (turn {}){reason} ═══",
            self.state.turn_number
        ));
    }

    // ── helpers ──────────────────────────────────────────────────────────────────────────────

    /// Look up a card by name/alias in the attached starter DB, returning its characteristics
    /// (with the right `grp_id`, so it casts/taps correctly).
    fn card_by_name(&self, name: &str) -> Option<Characteristics> {
        let grp = grp_for_name(name)?;
        self.state.card_db().get(grp).map(|d| d.chars.clone())
    }

    fn valid_player(&self, p: u32) -> bool {
        if (p as usize) < self.state.players.len() {
            true
        } else {
            self.say(&format!("no player {p} (game has {} players)", self.state.players.len()));
            false
        }
    }
}

/// Build the `GameState` for a `play` command from preset deck names (+ seed). `[]` = demo deck;
/// `["lands"]` = lands-only; one preset name = both seats; two = one per seat. `None` = unknown.
fn build_play_state(names: &[&str], seed: u64) -> Option<GameState> {
    match names {
        [] => Some(crate::driver::demo_state(seed)),
        ["lands"] => Some(crate::driver::lands_only_state(2, seed)),
        [a] => {
            let d = mtg_core::cards::preset_deck(a)?;
            Some(mtg_core::cards::build_game(seed, &[&d, &d]))
        }
        [a, b] => {
            let d0 = mtg_core::cards::preset_deck(a)?;
            let d1 = mtg_core::cards::preset_deck(b)?;
            Some(mtg_core::cards::build_game(seed, &[&d0, &d1]))
        }
        _ => None,
    }
}

/// Parse a turn-step name (for `stop <step>`).
fn parse_step(s: &str) -> Option<Phase> {
    Some(match s.to_ascii_lowercase().as_str() {
        "untap" => Phase::Untap,
        "upkeep" => Phase::Upkeep,
        "draw" => Phase::Draw,
        "mp1" | "main1" | "precombatmain" | "premain" => Phase::PrecombatMain,
        "begincombat" | "bc" => Phase::BeginCombat,
        "attackers" | "da" | "declareattackers" => Phase::DeclareAttackers,
        "blockers" | "db" | "declareblockers" => Phase::DeclareBlockers,
        "combatdamage" | "cd" | "damage" => Phase::CombatDamage,
        "endcombat" | "ec" => Phase::EndCombat,
        "mp2" | "main2" | "postcombatmain" | "postmain" => Phase::PostcombatMain,
        "end" | "endstep" => Phase::End,
        "cleanup" => Phase::Cleanup,
        _ => return None,
    })
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

/// A fresh game state with the starter card DB attached, so scenario cards cast/tap correctly.
fn fresh_state(num_players: usize, seed: u64) -> GameState {
    let mut state = GameState::new(num_players, seed);
    state.set_card_db(Arc::new(cards::starter_db()));
    state
}

/// Map a card name/alias to its starter-DB `grp_id` (case-insensitive). Aliases are single
/// tokens so multi-word names don't break whitespace-split arguments.
fn grp_for_name(name: &str) -> Option<u32> {
    use cards::grp;
    Some(match name.to_ascii_lowercase().as_str() {
        "plains" => grp::PLAINS,
        "island" => grp::ISLAND,
        "mountain" => grp::MOUNTAIN,
        "forest" => grp::FOREST,
        "bears" | "grizzly" => grp::GRIZZLY_BEARS,
        "giant" | "hill" => grp::HILL_GIANT,
        "shock" => grp::SHOCK,
        "divination" | "div" => grp::DIVINATION,
        "bolt" | "lightning" => grp::LIGHTNING_BOLT,
        "visionary" | "elvish" => grp::ELVISH_VISIONARY,
        "kavu" | "flametongue" | "ftk" => grp::FLAMETONGUE_KAVU,
        "anthem" | "glorious" | "gloriousanthem" => grp::GLORIOUS_ANTHEM,
        "levitation" | "levitate" => grp::LEVITATION,
        _ => return None,
    })
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
            Type 'help' for commands, or 'play' to start a game vs RandomAgent (creatures + combat).
            mtg> new 2 1
            new game: 2 players, seed 1 (P0 human, rest random)
            mtg> life 0 17
            P0 life = 17
            mtg> add 0 hand Forest Island
            added 2 card(s) to P0's Hand
            mtg> add 1 battlefield Mountain
            added 1 card(s) to P1's Battlefield
            mtg> deck 1 3
            added 3 card(s) to P1's library
            mtg> show
            === Turn 1 · Untap · active P0 · stack 0 · game_over false · winner — ===
            P0: life 17 · poison 0 · lands_played 0
                Hand       (2): Forest, Island
                Library    (0): (empty)
                Battlefield(0): (empty)
            P1: life 20 · poison 0 · lands_played 0
                Hand       (0): (empty)
                Library    (3): Plains, Island, Mountain
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
