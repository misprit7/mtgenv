//! M2 binary — serve the web board over HTTP + WebSocket.
//!
//! Run: `cargo run -p mtg-gre-server --bin mtg-serve` then open the printed URL.
//! Bind address override: `MTG_ADDR=0.0.0.0:9000 cargo run ... --bin mtg-serve`.
//!
//! Each browser connection plays one lands-only game (you = Player 0) vs a `RandomAgent`, all
//! through the same Agent boundary the CLI and RL backends use.

#[tokio::main]
async fn main() {
    let addr = std::env::var("MTG_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    if let Err(e) = mtg_gre_server::server::serve(&addr).await {
        eprintln!("mtg-serve: failed to bind/serve {addr}: {e}");
        std::process::exit(1);
    }
}
