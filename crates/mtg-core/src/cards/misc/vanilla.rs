//! Vanilla and French-vanilla (single-keyword) creatures — the starter bears/giant plus the
//! evergreen-keyword bodies from the #14 breadth pass. Keyword sets are Scryfall-verified.

use crate::basics::{CardType, Color};
use crate::cards::{grp, kw_creature, mana_cost, vanilla_creature, CardDb};
use crate::effects::ability::Keyword;

pub fn register(db: &mut CardDb) {
    db.insert(vanilla_creature(
        grp::GRIZZLY_BEARS,
        "Grizzly Bears",
        "Bear",
        Color::Green,
        mana_cost(1, &[(Color::Green, 1)]),
        2,
        2,
    ));
    db.insert(vanilla_creature(
        grp::HILL_GIANT,
        "Hill Giant",
        "Giant",
        Color::Red,
        mana_cost(3, &[(Color::Red, 1)]),
        3,
        3,
    ));
    // Evergreen-keyword creatures (Scryfall-verified single-keyword bodies).
    db.insert(kw_creature(grp::ELVISH_ARCHERS, "Elvish Archers", "Elf Archer", Color::Green,
        mana_cost(1, &[(Color::Green, 1)]), 2, 1, vec![Keyword::FirstStrike]).with_text("First strike"));
    db.insert(kw_creature(grp::FENCING_ACE, "Fencing Ace", "Human Soldier", Color::White,
        mana_cost(1, &[(Color::White, 1)]), 1, 1, vec![Keyword::DoubleStrike]).with_text("Double strike"));
    db.insert(kw_creature(grp::ARGOTHIAN_SWINE, "Argothian Swine", "Boar", Color::Green,
        mana_cost(3, &[(Color::Green, 1)]), 3, 3, vec![Keyword::Trample]).with_text("Trample"));
    db.insert(kw_creature(grp::TYPHOID_RATS, "Typhoid Rats", "Rat", Color::Black,
        mana_cost(0, &[(Color::Black, 1)]), 1, 1, vec![Keyword::Deathtouch]).with_text("Deathtouch"));
    db.insert(kw_creature(grp::CHILD_OF_NIGHT, "Child of Night", "Vampire", Color::Black,
        mana_cost(1, &[(Color::Black, 1)]), 2, 1, vec![Keyword::Lifelink]).with_text("Lifelink"));
    db.insert(kw_creature(grp::ALABORN_GRENADIER, "Alaborn Grenadier", "Human Soldier", Color::White,
        mana_cost(0, &[(Color::White, 2)]), 2, 2, vec![Keyword::Vigilance]).with_text("Vigilance"));
    db.insert(kw_creature(grp::ALLEY_STRANGLER, "Alley Strangler", "Human Assassin", Color::Black,
        mana_cost(2, &[(Color::Black, 1)]), 2, 3, vec![Keyword::Menace]).with_text("Menace"));
    db.insert(kw_creature(grp::WALL_OF_STONE, "Wall of Stone", "Wall", Color::Red,
        mana_cost(1, &[(Color::Red, 2)]), 0, 8, vec![Keyword::Defender]).with_text("Defender"));
    db.insert(kw_creature(grp::RAGING_GOBLIN, "Raging Goblin", "Goblin", Color::Red,
        mana_cost(0, &[(Color::Red, 1)]), 1, 1, vec![Keyword::Haste]).with_text("Haste"));
    db.insert(kw_creature(grp::KING_CHEETAH, "King Cheetah", "Cat", Color::Green,
        mana_cost(3, &[(Color::Green, 1)]), 3, 2, vec![Keyword::Flash]).with_text("Flash"));
    db.insert(kw_creature(grp::GLADECOVER_SCOUT, "Gladecover Scout", "Elf Scout", Color::Green,
        mana_cost(0, &[(Color::Green, 1)]), 1, 1, vec![Keyword::Hexproof]).with_text("Hexproof"));
    // Darksteel Myr — colorless Artifact Creature, indestructible.
    let mut myr = kw_creature(grp::DARKSTEEL_MYR, "Darksteel Myr", "Myr", Color::White,
        mana_cost(3, &[]), 0, 1, vec![Keyword::Indestructible]);
    myr.chars.card_types = vec![CardType::Artifact, CardType::Creature];
    myr.chars.colors = Vec::new();
    db.insert(myr.with_text("Indestructible"));
}
