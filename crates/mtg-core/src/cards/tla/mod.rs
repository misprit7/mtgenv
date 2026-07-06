//! TLA (Avatar: The Last Airbender) — first-printing-set folder.

use crate::cards::CardDb;

pub mod ba_sing_se;
pub mod badgermole_cub;
pub mod earthbender_ascension;
pub mod shared_roots;

pub fn register(db: &mut CardDb) {
    ba_sing_se::register(db);
    badgermole_cub::register(db);
    earthbender_ascension::register(db);
    shared_roots::register(db);
}
