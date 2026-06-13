//! The starter card set — card behaviour as **data** (Characteristics + design's Effect-IR
//! abilities). The core never matches on card names; it interprets these definitions.
//!
//! Milestone 3 keeps this tiny (CLAUDE.md "Scope — first pass"): basic lands, two vanilla
//! creatures, a damage instant, a draw sorcery, a gain-life instant. A [`CardDef`] bundles a
//! card's [`Characteristics`] with its [`Ability`]s; a [`CardDb`] is the registry keyed by
//! `grp_id`. Game objects reference their definition through `chars.grp_id`, so the (non-
//! serializable, fn-pointer-bearing) ability data lives out of the serializable `GameState`.
//!
//! Mana abilities are represented engine-side for now (a land "taps for one of these colors")
//! rather than via the full `Ability::Activated{is_mana}` IR — the minimal slice the M3
//! auto-tap payment needs (the IR path stays open).

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::basics::{Color, DamageKind, ManaCost, Zone};
use crate::effects::ability::Ability;
use crate::effects::target::{TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::ids::PlayerId;
use crate::state::{CardType, Characteristics, GameState};

/// Oracle/printing ids for the starter set (the `grp_id` linking an object to its [`CardDef`]).
pub mod grp {
    pub const PLAINS: u32 = 1;
    pub const ISLAND: u32 = 2;
    pub const MOUNTAIN: u32 = 3;
    pub const FOREST: u32 = 4;
    pub const GRIZZLY_BEARS: u32 = 10;
    pub const HILL_GIANT: u32 = 11;
    pub const SHOCK: u32 = 20;
    pub const DIVINATION: u32 = 21;
    pub const HEALING_SALVE: u32 = 22;
}

/// A card definition: its printed characteristics + abilities (the Effect IR), plus the
/// engine-side mana colours a land taps for. Card *data*, not game state — holds `Effect`
/// trees, so it is `Debug`/`Clone` but not serde.
#[derive(Debug, Clone)]
pub struct CardDef {
    pub chars: Characteristics,
    pub abilities: Vec<Ability>,
    /// Non-empty ⇒ this permanent has a "{T}: add one mana of one of these colours" ability
    /// (CR 605). Empty for non-mana cards.
    pub mana_colors: Vec<Color>,
}

impl CardDef {
    /// The spell ability's effect (CR 113.3a), if this card has one (instants/sorceries).
    pub fn spell_effect(&self) -> Option<&Effect> {
        self.abilities.iter().find_map(|a| match a {
            Ability::Spell { effect } => Some(effect),
            _ => None,
        })
    }
    pub fn is_mana_source(&self) -> bool {
        !self.mana_colors.is_empty()
    }
}

/// The card registry, keyed by `grp_id`. Default = empty.
#[derive(Debug, Clone, Default)]
pub struct CardDb {
    defs: BTreeMap<u32, CardDef>,
}

impl CardDb {
    pub fn get(&self, grp_id: u32) -> Option<&CardDef> {
        self.defs.get(&grp_id)
    }
    pub fn insert(&mut self, def: CardDef) {
        self.defs.insert(def.chars.grp_id, def);
    }
    pub fn len(&self) -> usize {
        self.defs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

fn mana_cost(generic: u32, pips: &[(Color, u32)]) -> ManaCost {
    let mut colored = BTreeMap::new();
    for &(c, n) in pips {
        *colored.entry(c).or_insert(0) += n;
    }
    ManaCost { generic, colored }
}

fn basic_land(grp_id: u32, name: &str, color: Color) -> CardDef {
    let mut chars = Characteristics::basic_land(name);
    chars.grp_id = grp_id;
    chars.colors = Vec::new(); // lands are colorless (CR 105.2a)
    CardDef {
        chars,
        abilities: Vec::new(),
        mana_colors: vec![color],
    }
}

fn vanilla_creature(
    grp_id: u32,
    name: &str,
    subtype: &str,
    color: Color,
    cost: ManaCost,
    power: i32,
    toughness: i32,
) -> CardDef {
    CardDef {
        chars: Characteristics {
            name: name.to_string(),
            card_types: vec![CardType::Creature],
            subtypes: vec![subtype.to_string()],
            colors: vec![color],
            mana_cost: Some(cost),
            power: Some(power),
            toughness: Some(toughness),
            grp_id,
            ..Default::default()
        },
        abilities: Vec::new(),
        mana_colors: Vec::new(),
    }
}

fn spell(grp_id: u32, name: &str, ty: CardType, color: Color, cost: ManaCost, effect: Effect) -> CardDef {
    CardDef {
        chars: Characteristics {
            name: name.to_string(),
            card_types: vec![ty],
            colors: vec![color],
            mana_cost: Some(cost),
            grp_id,
            ..Default::default()
        },
        abilities: vec![Ability::Spell { effect }],
        mana_colors: Vec::new(),
    }
}

/// "deal N to any target" (CR 115.4 "any target") — one target, locked at cast.
fn deal_to_any(amount: i64) -> Effect {
    Effect::DealDamage {
        amount: ValueExpr::Fixed(amount),
        to: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Any,
            min: 1,
            max: 1,
            distinct: true,
        }),
        kind: DamageKind::Noncombat,
    }
}

/// Build the starter card registry.
pub fn starter_db() -> CardDb {
    let mut db = CardDb::default();
    db.insert(basic_land(grp::PLAINS, "Plains", Color::White));
    db.insert(basic_land(grp::ISLAND, "Island", Color::Blue));
    db.insert(basic_land(grp::MOUNTAIN, "Mountain", Color::Red));
    db.insert(basic_land(grp::FOREST, "Forest", Color::Green));
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
    db.insert(spell(
        grp::SHOCK,
        "Shock",
        CardType::Instant,
        Color::Red,
        mana_cost(0, &[(Color::Red, 1)]),
        deal_to_any(2),
    ));
    db.insert(spell(
        grp::DIVINATION,
        "Divination",
        CardType::Sorcery,
        Color::Blue,
        mana_cost(2, &[(Color::Blue, 1)]),
        Effect::Draw {
            who: PlayerRef::Controller,
            count: ValueExpr::Fixed(2),
        },
    ));
    db.insert(spell(
        grp::HEALING_SALVE,
        "Healing Salve",
        CardType::Instant,
        Color::White,
        mana_cost(0, &[(Color::White, 1)]),
        Effect::GainLife {
            who: PlayerRef::Controller,
            amount: ValueExpr::Fixed(3),
        },
    ));
    db
}

/// A 30-card mono-aggressive Gruul (R/G) demo deck: plenty of mana, vanilla creatures to
/// attack with, and a couple of burn spells — reliably ends a `RandomAgent` game via combat
/// life loss (and exercises casting + the stack + combat + SBAs).
pub fn demo_deck() -> Vec<u32> {
    use std::iter::repeat;
    let mut deck = Vec::new();
    deck.extend(repeat(grp::FOREST).take(11));
    deck.extend(repeat(grp::MOUNTAIN).take(11));
    deck.extend(repeat(grp::GRIZZLY_BEARS).take(4));
    deck.extend(repeat(grp::HILL_GIANT).take(2));
    deck.extend(repeat(grp::SHOCK).take(2));
    deck
}

/// Build a two-player game from the demo deck with the starter [`CardDb`] attached.
pub fn two_player_demo_game(seed: u64) -> GameState {
    build_game(seed, &[&demo_deck(), &demo_deck()])
}

/// Build a game: one library per seat from a list of `grp_id` decks, with `starter_db()`
/// attached. Cards are added to libraries in deck order (the engine shuffles at game start).
pub fn build_game(seed: u64, decks: &[&[u32]]) -> GameState {
    let mut state = GameState::new(decks.len(), seed);
    state.set_card_db(Arc::new(starter_db()));
    let db = state.card_db();
    for (seat, deck) in decks.iter().enumerate() {
        let cards: Vec<Characteristics> = deck
            .iter()
            .filter_map(|&g| db.get(g).map(|d| d.chars.clone()))
            .collect();
        for chars in cards {
            state.add_card(PlayerId(seat as u32), chars, Zone::Library);
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_db_has_expected_cards() {
        let db = starter_db();
        assert_eq!(db.len(), 9);
        assert!(db.get(grp::FOREST).unwrap().is_mana_source());
        assert_eq!(db.get(grp::FOREST).unwrap().mana_colors, vec![Color::Green]);
        // Grizzly Bears is a vanilla 2/2 with no abilities.
        let bears = db.get(grp::GRIZZLY_BEARS).unwrap();
        assert_eq!(bears.chars.power, Some(2));
        assert!(bears.abilities.is_empty());
        assert!(!bears.is_mana_source());
        // Shock is an instant with a spell ability.
        assert!(db.get(grp::SHOCK).unwrap().spell_effect().is_some());
    }

    #[test]
    fn demo_deck_is_thirty_cards() {
        assert_eq!(demo_deck().len(), 30);
    }
}
