//! Time Spiral (`tsp`) — cards whose first real-expansion printing is TSP (in the SoS pool as reprints).

pub mod empty_the_warrens;
pub mod living_end;
pub mod smallpox;
pub mod terramorphic_expanse;

pub fn register(db: &mut super::CardDb) {
    empty_the_warrens::register(db);
    living_end::register(db);
    smallpox::register(db);
    terramorphic_expanse::register(db);
}
