//! mtga-bridge entrypoint.
//!
//! Boots the stub backend the real MTGA client connects to:
//!   1. load-or-generate the dev CA + leaf TLS cert (cached under `.certs/`),
//!   2. print the CA path + per-OS trust-install instructions,
//!   3. spawn the HTTPS login/doorbell stub (:443) and the FrontDoor TLS server
//!      (:FD_PORT, default 27000) concurrently, and run forever.
//!
//! Binding :443 needs root — run with sudo. Install the printed CA into your OS
//! trust store and point the MTGA hosts at 127.0.0.1 (see scripts/redirect.py)
//! before launching the client.
//!
//! Ports are configurable via args or env:
//!   - HTTPS:     `--https-port N`  or `MTGA_BRIDGE_HTTPS_PORT`   (default 443)
//!   - FrontDoor: `--fd-port N`     or `MTGA_BRIDGE_FD_PORT`      (default 27000)
//!   - GRE (stub):`--gre-port N`    or `MTGA_BRIDGE_GRE_PORT`     (default 27001)

use std::path::PathBuf;
use std::process::ExitCode;

use mtga_bridge::cert;
use mtga_bridge::frontdoor::{self, FrontdoorConfig};
use mtga_bridge::http_stub::{self, HttpConfig};

/// Parsed ports.
struct Config {
    https_port: u16,
    fd_port: u16,
    gre_port: u16,
}

fn parse_config() -> Config {
    let mut cfg = Config {
        https_port: env_port("MTGA_BRIDGE_HTTPS_PORT", 443),
        fd_port: env_port("MTGA_BRIDGE_FD_PORT", 27000),
        gre_port: env_port("MTGA_BRIDGE_GRE_PORT", 27001),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--https-port" => cfg.https_port = next_port(&mut args, cfg.https_port),
            "--fd-port" => cfg.fd_port = next_port(&mut args, cfg.fd_port),
            "--gre-port" => cfg.gre_port = next_port(&mut args, cfg.gre_port),
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            other => eprintln!("[main] ignoring unknown arg: {other}"),
        }
    }
    cfg
}

fn env_port(key: &str, default: u16) -> u16 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn next_port(args: &mut impl Iterator<Item = String>, fallback: u16) -> u16 {
    args.next().and_then(|v| v.parse().ok()).unwrap_or(fallback)
}

fn print_usage() {
    eprintln!("mtga-bridge — stub backend for the real MTGA client (interop research)");
    eprintln!();
    eprintln!("USAGE: mtga-bridge [--https-port N] [--fd-port N] [--gre-port N]");
    eprintln!("  --https-port N   HTTPS login/doorbell stub port (default 443; needs root)");
    eprintln!("  --fd-port N      FrontDoor TLS port (default 27000)");
    eprintln!("  --gre-port N     GRE endpoint advertised on match creation (default 27001)");
}

#[tokio::main]
async fn main() -> ExitCode {
    let cfg = parse_config();

    // File logging (so the bridge can be tailed without capturing a TTY).
    let log_path = std::env::var("MTGA_BRIDGE_LOG").unwrap_or_else(|_| "/tmp/mtga-bridge.log".to_string());
    mtga_bridge::logging::init(&log_path);

    // The crate root holds the `.certs/` cache. `CARGO_MANIFEST_DIR` is set at
    // build time; fall back to the current dir for an installed binary.
    let base_dir = manifest_dir();

    let bundle = match cert::load_or_generate(&base_dir) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[main] FATAL: could not load/generate TLS certs: {e}");
            return ExitCode::FAILURE;
        }
    };

    print_trust_instructions(&bundle.ca_cert_path);

    let server_config = match cert::server_config(&bundle) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[main] FATAL: could not build TLS server config: {e}");
            return ExitCode::FAILURE;
        }
    };

    let https_acceptor = http_stub::acceptor_from(server_config.clone());
    let fd_acceptor = frontdoor::acceptor_from(server_config);

    let http_cfg = HttpConfig { https_port: cfg.https_port, frontdoor_port: cfg.fd_port };
    let fd_cfg = FrontdoorConfig { frontdoor_port: cfg.fd_port, gre_port: cfg.gre_port };

    mtga_bridge::logging::log("main", &format!(
        "starting: HTTPS :{}  FrontDoor :{}  (GRE endpoint :{})  | logging to {}",
        cfg.https_port, cfg.fd_port, cfg.gre_port, log_path));

    let https = tokio::spawn(async move {
        if let Err(e) = http_stub::run(https_acceptor, http_cfg).await {
            eprintln!("[main] HTTPS stub exited with error: {e}");
        }
    });
    let fd = tokio::spawn(async move {
        if let Err(e) = frontdoor::run(fd_acceptor, fd_cfg).await {
            eprintln!("[main] FrontDoor exited with error: {e}");
        }
    });

    // Run until either task ends (they normally loop forever).
    let _ = tokio::try_join!(https, fd);
    ExitCode::SUCCESS
}

/// The crate root (compile-time manifest dir, falling back to cwd).
fn manifest_dir() -> PathBuf {
    option_env!("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// Print the absolute CA path and per-OS trust-store install instructions.
fn print_trust_instructions(ca_path: &std::path::Path) {
    let abs = ca_path.canonicalize().unwrap_or_else(|_| ca_path.to_path_buf());
    let p = abs.display();
    eprintln!("====================================================================");
    eprintln!(" mtga-bridge dev CA written to:");
    eprintln!("   {p}");
    eprintln!();
    eprintln!(" Install this CA into your OS trust store so the client trusts our");
    eprintln!(" certs (do this once; the CA is cached across runs):");
    eprintln!();
    eprintln!("  Linux (Arch/Fedora/openSUSE — p11-kit):");
    eprintln!("    sudo trust anchor '{p}'");
    eprintln!("  Linux (Debian/Ubuntu):");
    eprintln!("    sudo cp '{p}' /usr/local/share/ca-certificates/mtga-bridge-ca.crt && sudo update-ca-certificates");
    eprintln!();
    eprintln!("  macOS:");
    eprintln!("    sudo security add-trusted-cert -d -r trustRoot \\");
    eprintln!("      -k /Library/Keychains/System.keychain '{p}'");
    eprintln!();
    eprintln!("  Windows (Admin PowerShell/cmd):");
    eprintln!("    certutil -addstore -f Root \"{p}\"");
    eprintln!();
    eprintln!("  NOTE: running MTGA under Steam/Proton? This host CA propagates into");
    eprintln!("  the Wine prefix's trust store, which is what the client's UnityTLS");
    eprintln!("  validates the FrontDoor connection against. Fully restart MTGA after.");
    eprintln!("====================================================================");
}
