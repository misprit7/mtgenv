//! M1 binary — the expressive interactive / scriptable CLI.
//!
//! Run interactively:   `cargo run -p mtg-gre-server --bin mtg-play`
//!   then type `play` to start a lands-only game vs `RandomAgent`, or `help` for the full
//!   scenario-setup + inspection command set.
//! Run a script:        `cargo run -p mtg-gre-server --bin mtg-play -- --script game.txt`
//!   (setup commands + decision lines from a file → deterministic; pairs with expect-tests).
//!
//! Proves "a human is just another Agent": human seats are [`HumanAgent`]s behind the one
//! decision boundary; everything else (turn structure, priority, masking) is `mtg-core`.
//!
//! [`HumanAgent`]: mtg_gre_server::human::HumanAgent

use std::fs::File;
use std::io::{stdin, stdout, BufRead, BufReader};

use mtg_gre_server::cli;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut script: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--script" | "-s" => {
                script = args.get(i + 1).cloned();
                i += 2;
            }
            "--help" | "-h" => {
                eprintln!("usage: mtg-play [--script <file>]   (no args = interactive REPL on stdin)");
                return;
            }
            other => {
                eprintln!("mtg-play: unknown arg '{other}' (try --help)");
                i += 1;
            }
        }
    }

    let (reader, echo): (Box<dyn BufRead>, bool) = match &script {
        Some(path) => match File::open(path) {
            Ok(f) => (Box::new(BufReader::new(f)), true),
            Err(e) => {
                eprintln!("mtg-play: cannot open script '{path}': {e}");
                std::process::exit(1);
            }
        },
        None => (Box::new(stdin().lock()), false),
    };

    let io = cli::CliIo::new(reader, Box::new(stdout()), echo);
    cli::run(io);
}
