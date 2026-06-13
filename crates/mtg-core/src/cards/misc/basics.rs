//! The five basic lands (CR 305.6). Mana is intrinsic — the engine derives each one's `{T}: Add
//! <colour>` from its basic land subtype (Forest → {G}, …), so they're authored as type line only
//! (Basic Land + subtype), with no `mana_colors` shortcut and no explicit mana ability.

use crate::cards::{basic_land, grp, CardDb};

pub fn register(db: &mut CardDb) {
    db.insert(basic_land(grp::PLAINS, "Plains").with_text("({T}: Add {W}.)"));
    db.insert(basic_land(grp::ISLAND, "Island").with_text("({T}: Add {U}.)"));
    db.insert(basic_land(grp::MOUNTAIN, "Mountain").with_text("({T}: Add {R}.)"));
    db.insert(basic_land(grp::FOREST, "Forest").with_text("({T}: Add {G}.)"));
}
