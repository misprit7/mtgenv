//! The five basic lands (CR 305.6). Each taps for its one colour (engine-side `mana_colors`).

use crate::basics::Color;
use crate::cards::{basic_land, grp, CardDb};

pub fn register(db: &mut CardDb) {
    db.insert(basic_land(grp::PLAINS, "Plains", Color::White).with_text("({T}: Add {W}.)"));
    db.insert(basic_land(grp::ISLAND, "Island", Color::Blue).with_text("({T}: Add {U}.)"));
    db.insert(basic_land(grp::MOUNTAIN, "Mountain", Color::Red).with_text("({T}: Add {R}.)"));
    db.insert(basic_land(grp::FOREST, "Forest", Color::Green).with_text("({T}: Add {G}.)"));
}
