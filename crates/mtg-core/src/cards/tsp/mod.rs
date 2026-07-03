//! Time Spiral (`tsp`) — cards whose first real-expansion printing is TSP (in the SoS pool as reprints).

pub mod terramorphic_expanse;

pub fn register(db: &mut super::CardDb) {
    terramorphic_expanse::register(db);
}
