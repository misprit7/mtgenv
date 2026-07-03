//! Dump every registered card — `(grp_id, exact Scryfall name)` for the **full engine registry** —
//! as JSON on stdout. This is the source of truth the art resolver (`resolve-card-art.py`) reads, so
//! every implemented card (not just cards used by a preset) gets art fetched: no hand-maintained
//! grp_id→name list to keep in sync with the engine, and no card is invisible to the deck builder
//! for lack of art.
//!
//! Run: `cargo run -q -p mtg-gre-server --bin dump-cards`
//! Output: `[{"grp_id": <u32>, "name": "<exact card name>"}, …]` (sorted by grp_id).

fn main() {
    let entries: Vec<_> = mtg_gre_server::driver::all_cards()
        .into_iter()
        .map(|(grp_id, name)| serde_json::json!({ "grp_id": grp_id, "name": name }))
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&entries).expect("serialize card pool")
    );
}
