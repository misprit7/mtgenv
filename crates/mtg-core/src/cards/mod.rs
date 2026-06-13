//! Card data — card behaviour as **data** (Characteristics + design's Effect-IR abilities). The
//! core never matches on card names; it interprets these definitions.
//!
//! Organization (per the card-push spec): this module owns the [`CardDef`]/[`CardDb`] types, the
//! card *builders* (`creature`/`spell`/`aura`/…), the `grp::` id constants, and the deck builders.
//! The card *definitions* live in submodules — [`misc`] for the prototype/starter pool (grouped by
//! mechanic), and `<setcode>/` folders for real cards keyed by their first-printing set.
//! [`starter_db`] aggregates them all.
//!
//! A [`CardDef`] bundles a card's [`Characteristics`] with its [`Ability`]s; a [`CardDb`] is the
//! registry keyed by `grp_id`. Game objects reference their definition through `chars.grp_id`, so
//! the (non-serializable, fn-pointer-bearing) ability data lives out of the serializable
//! `GameState`. Mana abilities are represented engine-side for now (a land "taps for one of these
//! colours") rather than via the full `Ability::Activated{is_mana}` IR.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::basics::{CardType, Color, DamageKind, ManaCost, Zone};
use crate::effects::ability::{Ability, Cost, CostComponent, Keyword, Timing};
use crate::effects::target::{ManaSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::ids::PlayerId;
use crate::state::{Characteristics, GameState};

/// Shared card-construction fragments (`CardFilter`/`SelectSpec`/`ValueExpr` pieces). Card modules
/// import from here, never from a sibling card module.
pub mod helpers;

pub mod misc;

// Per-first-printing-set folders (real card pool).
pub mod dft;
pub mod dsk;
pub mod eoe;
pub mod fdn;
pub mod fin;
pub mod lea;

/// Oracle/printing ids (the `grp_id` linking an object to its [`CardDef`]). Per-set card ids move
/// near their cards in the `<setcode>/` folders; the prototype/starter ids stay here.
pub mod grp {
    pub const PLAINS: u32 = 1;
    pub const ISLAND: u32 = 2;
    pub const MOUNTAIN: u32 = 3;
    pub const FOREST: u32 = 4;
    pub const GRIZZLY_BEARS: u32 = 10;
    pub const HILL_GIANT: u32 = 11;
    pub const SHOCK: u32 = 20;
    pub const DIVINATION: u32 = 21;
    pub const LIGHTNING_BOLT: u32 = 23;
    // M4 prototype cards (triggers + replacement effects).
    pub const ELVISH_VISIONARY: u32 = 30;
    pub const FLAMETONGUE_KAVU: u32 = 31;
    pub const EXULTANT_CULTIST: u32 = 34;
    pub const ROOT_MAZE: u32 = 35;
    pub const HARDENED_SCALES: u32 = 36;
    // M5 layer-system cards (continuous effects).
    pub const GLORIOUS_ANTHEM: u32 = 40;
    pub const LEVITATION: u32 = 41;
    pub const NATURES_REVOLT: u32 = 43;
    // Evergreen-keyword creatures (#14).
    pub const ELVISH_ARCHERS: u32 = 50;
    pub const FENCING_ACE: u32 = 51;
    pub const ARGOTHIAN_SWINE: u32 = 52;
    pub const TYPHOID_RATS: u32 = 53;
    pub const CHILD_OF_NIGHT: u32 = 54;
    pub const ALABORN_GRENADIER: u32 = 55;
    pub const ALLEY_STRANGLER: u32 = 56;
    pub const WALL_OF_STONE: u32 = 57;
    pub const MURDER: u32 = 58;
    pub const DARKSTEEL_MYR: u32 = 59;
    pub const RAGING_GOBLIN: u32 = 60;
    pub const KING_CHEETAH: u32 = 61;
    pub const GLADECOVER_SCOUT: u32 = 62;
    pub const BONESPLITTER: u32 = 64;
    pub const PACIFISM: u32 = 65;
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
    /// Printed oracle/rules text for display (the view's `rules_text`). Reflects what the
    /// engine actually implements (Scryfall-verified where the implementation matches).
    pub text: String,
}

impl CardDef {
    /// Builder: set the display rules text.
    pub(crate) fn with_text(mut self, text: &str) -> Self {
        self.text = text.to_string();
        self
    }

    /// The spell ability's effect (CR 113.3a), if this card has one (instants/sorceries).
    pub fn spell_effect(&self) -> Option<&Effect> {
        self.abilities.iter().find_map(|a| match a {
            Ability::Spell { effect } => Some(effect),
            _ => None,
        })
    }
    pub fn is_mana_source(&self) -> bool {
        !self.mana_colors.is_empty()
            || self
                .abilities
                .iter()
                .any(|a| matches!(a, Ability::Activated { is_mana: true, .. }))
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

// ── Card builders (shared by the card-definition submodules) ──────────────────────────────────

pub(crate) fn mana_cost(generic: u32, pips: &[(Color, u32)]) -> ManaCost {
    let mut colored = BTreeMap::new();
    for &(c, n) in pips {
        *colored.entry(c).or_insert(0) += n;
    }
    ManaCost { generic, colored, x: 0 }
}

/// A plain `{T}: Add {C}` mana ability (CR 605) as first-class Effect IR — the canonical way to
/// give a land or dork its mana (replacing the legacy `mana_colors` shortcut). The engine reads
/// `Ability::Activated{is_mana:true}` + `Effect::AddMana` to offer the colour and fill the pool.
/// For conditional/any-color/choice mana, build the `Activated` directly with a `restriction` or
/// a richer `ManaSpec` (`any_color` / `one_of`).
pub(crate) fn mana_ability(color: Color) -> Ability {
    Ability::Activated {
        cost: Cost {
            mana: None,
            components: vec![CostComponent::TapSelf],
        },
        effect: Effect::AddMana {
            who: PlayerRef::Controller,
            mana: ManaSpec {
                produces: vec![(color, ValueExpr::Fixed(1))],
                any_color: None,
            },
        },
        timing: Timing::Instant,
        restriction: None,
        is_mana: true,
    }
}

pub(crate) fn basic_land(grp_id: u32, name: &str) -> CardDef {
    let mut chars = Characteristics::basic_land(name);
    chars.grp_id = grp_id;
    chars.colors = Vec::new(); // lands are colorless (CR 105.2a)
    CardDef {
        chars,
        abilities: Vec::new(),
        // Mana is intrinsic: the engine derives `{T}: Add <colour>` from the basic land subtype
        // (CR 305.6, e.g. Forest → {G}) — no `mana_colors` shortcut, no explicit mana ability.
        mana_colors: Vec::new(),
        text: String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn creature(
    grp_id: u32,
    name: &str,
    subtype: &str,
    color: Color,
    cost: ManaCost,
    power: i32,
    toughness: i32,
    abilities: Vec<Ability>,
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
        abilities,
        mana_colors: Vec::new(),
        text: String::new(),
    }
}

pub(crate) fn vanilla_creature(
    grp_id: u32,
    name: &str,
    subtype: &str,
    color: Color,
    cost: ManaCost,
    power: i32,
    toughness: i32,
) -> CardDef {
    creature(grp_id, name, subtype, color, cost, power, toughness, Vec::new())
}

/// A creature with printed keyword abilities (CR 702) and no other abilities.
#[allow(clippy::too_many_arguments)]
pub(crate) fn kw_creature(
    grp_id: u32,
    name: &str,
    subtype: &str,
    color: Color,
    cost: ManaCost,
    power: i32,
    toughness: i32,
    keywords: Vec<Keyword>,
) -> CardDef {
    let mut def = creature(grp_id, name, subtype, color, cost, power, toughness, Vec::new());
    def.chars.keywords = keywords;
    def
}

pub(crate) fn enchantment(grp_id: u32, name: &str, color: Color, cost: ManaCost, abilities: Vec<Ability>) -> CardDef {
    CardDef {
        chars: Characteristics {
            name: name.to_string(),
            card_types: vec![CardType::Enchantment],
            colors: vec![color],
            mana_cost: Some(cost),
            grp_id,
            ..Default::default()
        },
        abilities,
        mana_colors: Vec::new(),
        text: String::new(),
    }
}

/// An Aura (CR 303): an Enchantment with the "Aura" subtype. The engine reads the subtype to
/// require an enchant target at cast and to enter the battlefield attached (CR 303.4f / 608.3e).
pub(crate) fn aura(grp_id: u32, name: &str, color: Color, cost: ManaCost, abilities: Vec<Ability>) -> CardDef {
    let mut def = enchantment(grp_id, name, color, cost, abilities);
    def.chars.subtypes = vec!["Aura".to_string()];
    def
}

pub(crate) fn spell(grp_id: u32, name: &str, ty: CardType, color: Color, cost: ManaCost, effect: Effect) -> CardDef {
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
        text: String::new(),
    }
}

/// "deal N to any target" (CR 115.4 "any target") — one target, locked at cast.
pub(crate) fn deal_to_any(amount: i64) -> Effect {
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

/// Build the full card registry (all submodules).
pub fn starter_db() -> CardDb {
    let mut db = CardDb::default();
    misc::register(&mut db);
    // Per-set real cards (Selesnya Landfall push).
    lea::register(&mut db);
    dsk::register(&mut db);
    fin::register(&mut db);
    fdn::register(&mut db);
    eoe::register(&mut db);
    dft::register(&mut db);
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

/// "Burn": 40 Lightning Bolt + 20 Mountain (CR — a mono-red burn deck). Exercises
/// instant-speed casting, "any target" (face or creature), and the `DealDamage` runtime.
pub fn burn_deck() -> Vec<u32> {
    use std::iter::repeat;
    let mut deck = Vec::new();
    deck.extend(repeat(grp::LIGHTNING_BOLT).take(40));
    deck.extend(repeat(grp::MOUNTAIN).take(20));
    deck
}

/// "Bears": 40 Grizzly Bears + 20 Forest. Exercises sorcery-speed creature casting and
/// combat (attack / block / damage / lethal-damage SBA).
pub fn bears_deck() -> Vec<u32> {
    use std::iter::repeat;
    let mut deck = Vec::new();
    deck.extend(repeat(grp::GRIZZLY_BEARS).take(40));
    deck.extend(repeat(grp::FOREST).take(20));
    deck
}

/// A preset deck by name (`"burn"`, `"bears"`, `"demo"`), case-insensitive. For the harness/CLI.
pub fn preset_deck(name: &str) -> Option<Vec<u32>> {
    match name.to_ascii_lowercase().as_str() {
        "burn" => Some(burn_deck()),
        "bears" => Some(bears_deck()),
        "demo" => Some(demo_deck()),
        _ => None,
    }
}

/// Build a two-player game from the demo deck with the starter [`CardDb`] attached.
pub fn two_player_demo_game(seed: u64) -> GameState {
    build_game(seed, &[&demo_deck(), &demo_deck()])
}

/// The user's hand-test matchup: seat 0 plays Burn, seat 1 plays Bears.
pub fn burn_vs_bears_game(seed: u64) -> GameState {
    build_game(seed, &[&burn_deck(), &bears_deck()])
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
        assert_eq!(db.len(), 38);
        // Forest is "type line only": a Basic Land with subtype Forest. Mana is intrinsic
        // (CR 305.6) — the engine derives {T}: Add {G} from the subtype, so the CardDef carries
        // no mana_colors / mana ability.
        let forest = db.get(grp::FOREST).unwrap();
        assert_eq!(forest.chars.subtypes, vec!["Forest".to_string()]);
        assert!(forest.chars.supertypes.contains(&"Basic".to_string()));
        assert!(forest.mana_colors.is_empty());
        // Grizzly Bears is a vanilla 2/2 with no abilities.
        let bears = db.get(grp::GRIZZLY_BEARS).unwrap();
        assert_eq!(bears.chars.power, Some(2));
        assert!(bears.abilities.is_empty());
        assert!(!bears.is_mana_source());
        // Shock and Lightning Bolt are instants with a spell ability.
        assert!(db.get(grp::SHOCK).unwrap().spell_effect().is_some());
        assert!(db.get(grp::LIGHTNING_BOLT).unwrap().spell_effect().is_some());
        // Oracle text is carried for display (the view's rules_text); vanilla cards have none.
        assert_eq!(
            db.get(grp::LIGHTNING_BOLT).unwrap().text,
            "Lightning Bolt deals 3 damage to any target."
        );
        assert!(db.get(grp::GRIZZLY_BEARS).unwrap().text.is_empty());
    }

    #[test]
    fn decks_are_the_expected_sizes() {
        assert_eq!(demo_deck().len(), 30);
        assert_eq!(burn_deck().len(), 60);
        assert_eq!(bears_deck().len(), 60);
        assert_eq!(preset_deck("BURN").unwrap().len(), 60);
        assert!(preset_deck("nonesuch").is_none());
    }
}
