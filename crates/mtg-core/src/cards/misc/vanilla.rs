//! Vanilla and French-vanilla (single-keyword) creatures — the starter bears/giant plus the
//! evergreen-keyword bodies from the #14 breadth pass. Keyword sets are Scryfall-verified.

use crate::basics::{CardType, Color};
use crate::cards::{grp, kw_creature, mana_cost, vanilla_creature, CardDb};
use crate::effects::ability::Keyword;
use crate::subtypes::CreatureType;

pub fn register(db: &mut CardDb) {
    db.insert(vanilla_creature(
        grp::GRIZZLY_BEARS,
        "Grizzly Bears",
        CreatureType::Bear,
        Color::Green,
        mana_cost(1, &[(Color::Green, 1)]),
        2,
        2,
    ));
    db.insert(vanilla_creature(
        grp::HILL_GIANT,
        "Hill Giant",
        CreatureType::Giant,
        Color::Red,
        mana_cost(3, &[(Color::Red, 1)]),
        3,
        3,
    ));
    // Evergreen-keyword creatures (Scryfall-verified single-keyword bodies).
    db.insert(kw_creature(grp::ARGOTHIAN_SWINE, "Argothian Swine", CreatureType::Boar, Color::Green,
        mana_cost(3, &[(Color::Green, 1)]), 3, 3, vec![Keyword::Trample]).with_text("Trample"));
    db.insert(kw_creature(grp::TYPHOID_RATS, "Typhoid Rats", CreatureType::Rat, Color::Black,
        mana_cost(0, &[(Color::Black, 1)]), 1, 1, vec![Keyword::Deathtouch]).with_text("Deathtouch"));
    db.insert(kw_creature(grp::CHILD_OF_NIGHT, "Child of Night", CreatureType::Vampire, Color::Black,
        mana_cost(1, &[(Color::Black, 1)]), 2, 1, vec![Keyword::Lifelink]).with_text("Lifelink"));
    db.insert(kw_creature(grp::WALL_OF_STONE, "Wall of Stone", CreatureType::Wall, Color::Red,
        mana_cost(1, &[(Color::Red, 2)]), 0, 8, vec![Keyword::Defender]).with_text("Defender"));
    db.insert(kw_creature(grp::RAGING_GOBLIN, "Raging Goblin", CreatureType::Goblin, Color::Red,
        mana_cost(0, &[(Color::Red, 1)]), 1, 1, vec![Keyword::Haste]).with_text("Haste"));
    db.insert(kw_creature(grp::KING_CHEETAH, "King Cheetah", CreatureType::Cat, Color::Green,
        mana_cost(3, &[(Color::Green, 1)]), 3, 2, vec![Keyword::Flash]).with_text("Flash"));
    // Two-subtype bodies: the builder takes the primary type; set the full subtype line after
    // (CR 205.3 — a creature can have several subtypes, e.g. "Human Soldier").
    db.insert(two_subtype_kw(grp::ELVISH_ARCHERS, "Elvish Archers", CreatureType::Elf, CreatureType::Archer,
        Color::Green, mana_cost(1, &[(Color::Green, 1)]), 2, 1, Keyword::FirstStrike, "First strike"));
    db.insert(two_subtype_kw(grp::FENCING_ACE, "Fencing Ace", CreatureType::Human, CreatureType::Soldier,
        Color::White, mana_cost(1, &[(Color::White, 1)]), 1, 1, Keyword::DoubleStrike, "Double strike"));
    db.insert(two_subtype_kw(grp::ALABORN_GRENADIER, "Alaborn Grenadier", CreatureType::Human, CreatureType::Soldier,
        Color::White, mana_cost(0, &[(Color::White, 2)]), 2, 2, Keyword::Vigilance, "Vigilance"));
    db.insert(two_subtype_kw(grp::ALLEY_STRANGLER, "Alley Strangler", CreatureType::Human, CreatureType::Assassin,
        Color::Black, mana_cost(2, &[(Color::Black, 1)]), 2, 3, Keyword::Menace, "Menace"));
    db.insert(two_subtype_kw(grp::GLADECOVER_SCOUT, "Gladecover Scout", CreatureType::Elf, CreatureType::Scout,
        Color::Green, mana_cost(0, &[(Color::Green, 1)]), 1, 1, Keyword::Hexproof, "Hexproof"));
    // Darksteel Myr — colorless Artifact Creature, indestructible.
    let mut myr = kw_creature(grp::DARKSTEEL_MYR, "Darksteel Myr", CreatureType::Myr, Color::White,
        mana_cost(3, &[]), 0, 1, vec![Keyword::Indestructible]);
    myr.chars.card_types = vec![CardType::Artifact, CardType::Creature];
    myr.chars.colors = Vec::new();
    db.insert(myr.with_text("Indestructible"));
}

/// A single-keyword creature with a two-subtype type line (e.g. "Human Soldier"). The `creature`
/// builder takes one primary `CreatureType`; this sets the full `[primary, secondary]` subtype
/// line after construction (CR 205.3).
#[allow(clippy::too_many_arguments)]
fn two_subtype_kw(
    grp_id: u32,
    name: &str,
    primary: CreatureType,
    secondary: CreatureType,
    color: Color,
    cost: crate::basics::ManaCost,
    power: i32,
    toughness: i32,
    keyword: Keyword,
    text: &str,
) -> crate::cards::CardDef {
    let mut def = kw_creature(grp_id, name, primary, color, cost, power, toughness, vec![keyword]);
    def.chars.subtypes = vec![primary.into(), secondary.into()];
    def.with_text(text)
}
