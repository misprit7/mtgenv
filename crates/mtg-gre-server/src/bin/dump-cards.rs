//! Dump the card pool the web server can show — every `(grp_id, exact Scryfall name)` across all
//! selectable decks — as JSON on stdout. This is the **single source of truth** the art resolver
//! (`resolve-card-art.py`) reads, so adding a card to a deck automatically extends the art fetch:
//! no hand-maintained grp_id→name list to keep in sync with the engine.
//!
//! Run: `cargo run -q -p mtg-gre-server --bin dump-cards`
//! Output: `[{"grp_id": <u32>, "name": "<exact card name>"}, …]` (sorted by grp_id).

fn main() {
    let entries: Vec<_> = mtg_gre_server::driver::deck_card_pool()
        .into_iter()
        .map(|(grp_id, name)| serde_json::json!({ "grp_id": grp_id, "name": name }))
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&entries).expect("serialize card pool")
    );
}
